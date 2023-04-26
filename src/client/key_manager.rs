use ahash::HashMap;
use std::sync::Arc;
use chrono::{Duration, Utc};
use parking_lot::Mutex;
use crate::client::rate_limiter::MultiLimits;
use crate::config::config_model::MetricsConfig;


pub trait KeyManager {
    fn acquire_key(&self,
                   request_type: KeyType) -> anyhow::Result<Key>;
}

// struct responsible for keys, state of keys and rate limiting
#[derive(Clone)]
pub struct KeyManagerImpl {
    configs: MetricsConfig,
    state: Arc<Mutex<KeyManagerState>>,
}

const COUNT_OF_LIMITS: usize = 2;
type KeyLimit = MultiLimits<COUNT_OF_LIMITS>;
fn create_key_limit() -> KeyLimit {
    // see https://docs.digitalocean.com/reference/api/api-reference/#section/Introduction/Rate-Limit
    KeyLimit::new(
        [
            (250, Duration::minutes(1)),
            (2000, Duration::hours(1))
        ],
        Utc::now(),
    )
}

struct KeyManagerState {
    keys: HashMap<KeyType, Vec<Key>>,
    settings: HashMap<Key, KeyLimit>,
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


impl KeyManager for KeyManagerImpl {
    fn acquire_key(&self,
                   request_type: KeyType) -> anyhow::Result<String> {
        self.state.lock().acquire_key(request_type)
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
                        self.settings.get(k)
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
        if let Some(settings) = self.settings.get_mut(&key) {
            let _ = settings.acquire(current_time);
        }

        Ok(key)
    }
}

#[cfg(test)]
mod key_manager {
    #[test]
    fn acquire_key() {
        todo!()
    }

    #[test]
    fn fallback_to_second_key_on_limits() {
        todo!()
    }

    #[test]
    fn fallback_to_default_if_not_found() {
        todo!()
    }

    #[test]
    fn fallback_to_default_on_limits() {
        todo!()
    }

    #[test]
    fn fail_if_no_key_found() {
        todo!()
    }

    #[test]
    fn fail_on_rate_limit() {
        todo!()
    }
}