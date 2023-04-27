use ahash::HashMap;
use std::sync::Arc;
use chrono::{DateTime, Duration, Utc};
use parking_lot::Mutex;
use crate::client::rate_limiter::MultiLimits;
use crate::config::config_model::AppSettings;


pub trait KeyManager {
    fn acquire_key(&self,
                   key_type: KeyType) -> anyhow::Result<Key>;
}

// struct responsible for keys, state of keys and rate limiting
#[derive(Clone)]
pub struct KeyManagerImpl {
    state: Arc<Mutex<KeyManagerState>>,
}

impl KeyManagerImpl {
    fn new(configs: AppSettings) -> Self {
        Self {
            state: Arc::new(Mutex::new(KeyManagerState::new(configs)))
        }
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

impl KeyManagerState {
    pub fn new(mut configs: AppSettings) -> Self {
        let mut keys: HashMap<KeyType, Vec<Key>> = Default::default();

        keys.insert(KeyType::Default, std::mem::take(&mut configs.default_keys));
        keys.insert(KeyType::Droplets, std::mem::take(&mut configs.droplets.keys));
        if let Some(bandwidth) = configs.metrics.bandwidth.as_mut() {
            keys.insert(
                KeyType::Bandwidth,
                std::mem::take(&mut bandwidth.keys),
            );
        }
        if let Some(cpu) = configs.metrics.cpu.as_mut() {
            keys.insert(
                KeyType::Cpu,
                std::mem::take(&mut cpu.keys),
            );
        }
        if let Some(filesystem) = configs.metrics.filesystem.as_mut() {
            keys.insert(
                KeyType::FileSystem,
                std::mem::take(&mut filesystem.keys),
            );
        }
        if let Some(memory) = configs.metrics.memory.as_mut() {
            keys.insert(
                KeyType::Memory,
                std::mem::take(&mut memory.keys),
            );
        }
        if let Some(load) = configs.metrics.load.as_mut() {
            keys.insert(
                KeyType::Load,
                std::mem::take(&mut load.keys),
            );
        }


        // 10 minutes for small amount of initial limits
        let time: DateTime<Utc> = Utc::now() - Duration::minutes(10);
        let limits: HashMap<Key, KeyLimit> = keys.values()
            .flat_map(|keys| keys)
            .map(|k| (k.clone(), create_key_limit(time)))
            .collect();

        Self { keys, limits }
    }
}


impl KeyManagerState {
    fn acquire_key(&mut self,
                   request_type: KeyType) -> anyhow::Result<String> {
        let current_time = Utc::now();

        let key_result = match self.keys.get(&request_type) {
            None if request_type == KeyType::Default => anyhow::bail!("Api Key Not Found"),
            None => self.acquire_key(KeyType::Default),
            Some(keys) => {
                let available_key = keys.iter()
                    .flat_map(|k|
                        self.limits.get(k)
                            .map(|settings| (k, settings))
                    ).find(|(_, x)| x.can_acquire(current_time));
                match available_key {
                    None if request_type == KeyType::Default => anyhow::bail!("Available Api Key Not Found"),
                    None => self.acquire_key(KeyType::Default),
                    Some((key, _)) => Ok(key.clone())
                }
            }
        };

        let key = key_result?;
        if let Some(settings) = self.limits.get_mut(&key) {
            let _ = settings.acquire(current_time);
        }

        Ok(key)
    }
}

#[cfg(test)]
mod key_manager {
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

        let manager = KeyManagerImpl::new(configs);

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

        let manager = KeyManagerImpl::new(configs);

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

        let manager = KeyManagerImpl::new(configs);

        let key = manager.acquire_key(KeyType::Memory).unwrap();
        assert_eq!(key, "default".to_string());
    }

}