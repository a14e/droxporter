use crate::config::config_model::AppSettings;
use crate::config::env_expanding::expand_env_var;
use std::fs;

pub fn parse_configs(path: String) -> anyhow::Result<AppSettings> {
    let yml = fs::read_to_string(path)?;
    let yml = expand_env_var(yml.as_str())?;
    let result = serde_yaml::from_str(yml.as_str()).map_err(anyhow::Error::new)?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_parse_valid_config() {
        let mut temp_file = NamedTempFile::with_prefix("droxporter_valid_config_test_").unwrap();
        let config_content = r#"
endpoint:
  port: 9999
  host: "127.0.0.1"
exporter-metrics:
  enabled: true
  interval: 10s
  metrics:
    - cpu
    - memory
default-keys: ["test-key"]
"#;
        temp_file.write_all(config_content.as_bytes()).unwrap();

        let result = parse_configs(temp_file.path().to_str().unwrap().to_string());
        assert!(result.is_ok());

        let config = result.unwrap();
        assert_eq!(config.endpoint.port, 9999);
        assert_eq!(config.endpoint.host, "127.0.0.1");
        assert!(config.exporter_metrics.enabled);
        assert_eq!(config.exporter_metrics.metrics.len(), 2);
        assert_eq!(config.default_keys.len(), 1);
        assert_eq!(config.default_keys[0], "test-key");
    }

    #[test]
    fn test_parse_nonexistent_file() {
        let result = parse_configs("/nonexistent/path/config.yml".to_string());
        assert!(result.is_err());
        // Just check that it fails, don't depend on specific error message format
    }

    #[test]
    fn test_parse_invalid_yaml() {
        let mut temp_file = NamedTempFile::with_prefix("droxporter_invalid_yaml_test_").unwrap();
        let invalid_yaml = r#"
endpoint:
  port: 9999
  host: "127.0.0.1"
invalid_yaml: [
  unclosed_array
"#;
        temp_file.write_all(invalid_yaml.as_bytes()).unwrap();

        let result = parse_configs(temp_file.path().to_str().unwrap().to_string());
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_empty_file() {
        let mut temp_file = NamedTempFile::with_prefix("droxporter_empty_file_test_").unwrap();
        temp_file.write_all(b"").unwrap();

        let result = parse_configs(temp_file.path().to_str().unwrap().to_string());
        // Should parse successfully with default values
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_config_with_env_vars() {
        unsafe {
            std::env::set_var("TEST_PORT", "8888");
        }
        unsafe {
            std::env::set_var("TEST_HOST", "0.0.0.0");
        }

        let mut temp_file = NamedTempFile::new().unwrap();
        let config_content = r#"
endpoint:
  port: ${TEST_PORT}
  host: ${TEST_HOST}
"#;
        temp_file.write_all(config_content.as_bytes()).unwrap();

        let result = parse_configs(temp_file.path().to_str().unwrap().to_string());
        assert!(result.is_ok());

        let config = result.unwrap();
        assert_eq!(config.endpoint.port, 8888);
        assert_eq!(config.endpoint.host, "0.0.0.0");

        // Cleanup
        unsafe {
            std::env::remove_var("TEST_PORT");
            std::env::remove_var("TEST_HOST");
        }
    }
}
