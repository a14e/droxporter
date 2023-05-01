use ahash::{HashMap, HashSet};
use std::sync::Arc;
use chrono::{DateTime, Duration, Utc};
use parking_lot::Mutex;
use prometheus::{CounterVec, GaugeVec, Opts, Registry};
use crate::client::rate_limiter::MultiLimits;
use crate::config::config_model::AppSettings;


pub trait KeyManager: Send + Sync {
    fn acquire_key(&self,
                   key_type: KeyType) -> anyhow::Result<Key>;
}

// struct responsible for keys, state of keys and rate limiting
#[derive(Clone)]
pub struct KeyManagerImpl {
    state: Arc<Mutex<KeyManagerState>>,
}

impl KeyManagerImpl {
    pub fn new(configs: &AppSettings,
               registry: Registry) -> anyhow::Result<Self> {
        let result = Self {
            state: Arc::new(Mutex::new(KeyManagerState::new(configs, registry)?))
        };
        Ok(result)
    }
}

impl KeyManager for KeyManagerImpl {
    fn acquire_key(&self, key_type: KeyType) -> anyhow::Result<Key> {
        self.state.lock().acquire_key(key_type)
    }
}


const COUNT_OF_LIMITS: usize = 2;

type KeyLimit = MultiLimits<COUNT_OF_LIMITS>;

fn create_key_limit(time: DateTime<Utc>) -> KeyLimit {
    // see https://docs.digitalocean.com/reference/api/api-reference/#section/Introduction/Rate-Limit
    KeyLimit::new(
        [
            (250, Duration::minutes(1)),
            (2000, Duration::hours(1))
        ],
        time,
    )
}

struct KeyManagerState {
    keys: HashMap<KeyType, Vec<Key>>,
    limits: HashMap<Key, KeyLimit>,

    limits_gauge: GaugeVec,
    keys_status_gauge: GaugeVec,
    key_error_counter: CounterVec,
}

type Key = String;

#[derive(PartialEq, Eq, Hash, Copy, Clone)]
pub enum KeyType {
    Default,
    Droplets,
    Bandwidth,
    Cpu,
    FileSystem,
    Memory,
    Load,
}

impl KeyType {
    fn to_metric_type(&self) -> &'static str {
        match self {
            KeyType::Default => "default",
            KeyType::Droplets => "droplets",
            KeyType::Bandwidth => "bandwidth",
            KeyType::Cpu => "cpu",
            KeyType::FileSystem => "file_system",
            KeyType::Memory => "memory",
            KeyType::Load => "load",
        }
    }
}

impl KeyManagerState {
    pub fn new(configs: &AppSettings,
               registry: Registry) -> anyhow::Result<Self> {
        let mut keys: HashMap<KeyType, Vec<Key>> = Default::default();

        keys.insert(KeyType::Default, configs.default_keys.clone());
        keys.insert(KeyType::Droplets, configs.droplets.keys.clone());
        if let Some(bandwidth) = configs.metrics.bandwidth.as_ref() {
            keys.insert(
                KeyType::Bandwidth,
                bandwidth.keys.clone(),
            );
        }
        if let Some(cpu) = configs.metrics.cpu.as_ref() {
            keys.insert(
                KeyType::Cpu,
                cpu.keys.clone(),
            );
        }
        if let Some(filesystem) = configs.metrics.filesystem.as_ref() {
            keys.insert(
                KeyType::FileSystem,
                filesystem.keys.clone(),
            );
        }
        if let Some(memory) = configs.metrics.memory.as_ref() {
            keys.insert(
                KeyType::Memory,
                memory.keys.clone(),
            );
        }
        if let Some(load) = configs.metrics.load.as_ref() {
            keys.insert(
                KeyType::Load,
                load.keys.clone(),
            );
        }


        // 10 minutes for small amount of initial limits
        let time: DateTime<Utc> = Utc::now() - Duration::minutes(10);
        let limits: HashMap<Key, KeyLimit> = keys.values()
            .flat_map(|keys| keys)
            .map(|k| (k.clone(), create_key_limit(time)))
            .collect();


        let limits_gauge = GaugeVec::new(
            Opts::new("droxporter_remaining_limits_by_key", "Remaining attempts count per timeframe"),
            &["key_type", "timeframe"],
        )?;
        let keys_status_gauge = GaugeVec::new(
            Opts::new("droxporter_keys_count_by_status", "Count of keys by status"),
            &["key_type", "status"],
        )?;
        let key_error_counter = CounterVec::new(
            Opts::new("droxporter_keys_errors", "Key errors"),
            &["key_type", "error"],
        )?;
        registry.register(Box::new(limits_gauge.clone()))?;
        registry.register(Box::new(keys_status_gauge.clone()))?;
        registry.register(Box::new(key_error_counter.clone()))?;

        let result = Self { keys, limits, limits_gauge, keys_status_gauge, key_error_counter };
        Ok(result)
    }
}


impl KeyManagerState {
    fn acquire_key(&mut self,
                   key_type: KeyType) -> anyhow::Result<String> {
        let current_time = Utc::now();

        let key_result = match self.keys.get(&key_type) {
            None if key_type == KeyType::Default => anyhow::bail!("Api Key Not Found"),
            None => {
                // return is important here to prevent double acquiring
                return self.acquire_key(KeyType::Default)
            },
            Some(keys) => {
                let available_key = keys.iter()
                    .flat_map(|k|
                        self.limits.get(k)
                            .map(|settings| (k, settings))
                    ).filter(|(_, x)| x.can_acquire(current_time))
                    .max_by_key(|(_, x)| {
                        let one_minute_idx = 0;
                        let one_hour_idx = 1;
                        x.estimate_remaining(one_minute_idx, current_time) +
                            x.estimate_remaining(one_hour_idx, current_time)
                    });
                match available_key {
                    None if key_type == KeyType::Default => anyhow::bail!("Available Api Key Not Found Or Limit exceeded"),
                    None => self.acquire_key(KeyType::Default),
                    Some((key, _)) => Ok(key.clone())
                }
            }
        };


        if key_result.is_err() {
            // I think, that a bug is here. Because I always will record only Default key type
            self.record_fail(key_type)
        }

        let key = key_result?;
        if let Some(settings) = self.limits.get_mut(&key) {
            let _ = settings.acquire(current_time);
        }

        // Should I call this function separately? Actually, I don't believe it would cause any noticeable slowdowns
        self.record_metrics();

        Ok(key)
    }

    fn record_fail(&self, key_type: KeyType) {
        let key_not_found = self.keys.get(&key_type)
            .map(|x| x.is_empty())
            .unwrap_or(true);
        let default_key_not_found = self.keys.get(&KeyType::Default)
            .map(|x| x.is_empty())
            .unwrap_or(true);
        let error_type = if key_not_found && default_key_not_found {
            "key not found"
        } else {
            "limit exceeded"
        };
        self.key_error_counter
            .with_label_values(&[key_type.to_metric_type(), error_type])
            .inc()
    }

    fn record_metrics(&self) {
        let one_minute_idx = 0;
        let one_hour_idx = 1;

        for (elem, keys) in self.keys.iter() {
            let metric_type = elem.to_metric_type();
            let keys: HashSet<_> = keys.iter().collect();
            if keys.is_empty() {
                continue;
            }
            let time = Utc::now();
            let remaining_1_minute: usize = keys.iter()
                .flat_map(|k| self.limits.get(*k))
                .map(|l| l.estimate_remaining(one_minute_idx, time))
                .sum();

            self.limits_gauge
                .with_label_values(&[metric_type, "1 min"])
                .set(remaining_1_minute as f64);

            let remaining_1_hour: usize = keys.iter()
                .flat_map(|k| self.limits.get(*k))
                .map(|l| l.estimate_remaining(one_hour_idx, time))
                .sum();

            self.limits_gauge
                .with_label_values(&[metric_type, "1 hour"])
                .set(remaining_1_hour as f64);

            let active_keys = keys.iter()
                .flat_map(|k| self.limits.get(*k))
                .map(|l| l.can_acquire(time))
                .count();
            self.keys_status_gauge
                .with_label_values(&[metric_type, "active"])
                .set(active_keys as f64);
            let inactive_keys = keys.len() - active_keys;
            self.keys_status_gauge
                .with_label_values(&[metric_type, "exceeded"])
                .set(inactive_keys as f64);
        }
    }
}

#[cfg(test)]
mod key_manager {
    use prometheus::Registry;
    use crate::client::key_manager::{KeyManager, KeyManagerImpl, KeyType};
    use crate::config::config_model::AppSettings;

    #[test]
    fn acquire_key() {
        let mut configs = AppSettings::default();
        configs.metrics.memory = Some(Default::default());
        configs.metrics.cpu = Some(Default::default());
        configs.metrics.load = Some(Default::default());
        configs.metrics.filesystem = Some(Default::default());
        configs.metrics.bandwidth = Some(Default::default());

        configs.default_keys = vec!["default".into()];

        configs.metrics.memory.as_mut().unwrap().keys = vec!["memory".into()];
        configs.metrics.cpu.as_mut().unwrap().keys = vec!["cpu".into()];
        configs.metrics.load.as_mut().unwrap().keys = vec!["load".into()];
        configs.metrics.filesystem.as_mut().unwrap().keys = vec!["filesystem".into()];
        configs.metrics.bandwidth.as_mut().unwrap().keys = vec!["bandwidth".into()];
        configs.droplets.keys = vec!["droplets".into()];

        let manager = KeyManagerImpl::new(&configs, Registry::new()).unwrap();

        let key = manager.acquire_key(KeyType::Memory).unwrap();
        assert_eq!(key, "memory".to_string());
        let key = manager.acquire_key(KeyType::Cpu).unwrap();
        assert_eq!(key, "cpu".to_string());
        let key = manager.acquire_key(KeyType::Load).unwrap();
        assert_eq!(key, "load".to_string());
        let key = manager.acquire_key(KeyType::FileSystem).unwrap();
        assert_eq!(key, "filesystem".to_string());
        let key = manager.acquire_key(KeyType::Bandwidth).unwrap();
        assert_eq!(key, "bandwidth".to_string());
        let key = manager.acquire_key(KeyType::Default).unwrap();
        assert_eq!(key, "default".to_string());
        let key = manager.acquire_key(KeyType::Droplets).unwrap();
        assert_eq!(key, "droplets".to_string());
    }

    #[test]
    fn fallback_to_second_key_on_limits() {
        let mut configs = AppSettings::default();
        configs.metrics.memory = Some(Default::default());
        configs.default_keys = vec!["default".into()];

        configs.metrics.memory.as_mut().unwrap().keys = vec!["memory".into()];

        let manager = KeyManagerImpl::new(&configs, Registry::new()).unwrap();

        for _ in 0..250 {
            manager.acquire_key(KeyType::Memory).unwrap();
        }

        let key = manager.acquire_key(KeyType::Memory).unwrap();
        assert_eq!(key, "default".to_string());
    }

    #[test]
    fn fallback_to_default_if_not_found() {
        let mut configs = AppSettings::default();
        configs.metrics.memory = Some(Default::default());
        configs.default_keys = vec!["default".into()];

        let manager = KeyManagerImpl::new(&configs, Registry::new()).unwrap();
        ;

        let key = manager.acquire_key(KeyType::Memory).unwrap();
        assert_eq!(key, "default".to_string());
    }
}