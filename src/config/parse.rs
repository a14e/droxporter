use std::fs;
use crate::config::config_model::AppSettings;
use crate::config::env_expanding::expand_env_var;

pub fn parse_configs(path: String) -> anyhow::Result<AppSettings> {
    let yml = fs::read_to_string(path)?;
    let yml = expand_env_var(yml.as_str())?;
    let result = serde_yaml::from_str(yml.as_str())
        .map_err(|x| anyhow::Error::new(x))?;
    Ok(result)
}


