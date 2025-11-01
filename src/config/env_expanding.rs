use regex::{Captures, Regex};
use std::env;
use tracing::{error, info};

pub fn expand_env_var(raw_config: &str) -> anyhow::Result<String> {
    // Regular expression composed and explained by Chat GPT
    //
    //  The regular expression r"\$\{([a-zA-Z_][0-9a-zA-Z_]*)(?::([^}]*))?\}" is broken down into the following parts:
    //
    // \$\{ - the sequence of characters ${, which is the beginning of the replaceable substring.
    //
    // ([a-zA-Z_][0-9a-zA-Z_]*) - captures the variable name, which can consist of letters, digits,
    // and underscores, starting with a letter or an underscore.
    //
    // (?::([^}]*))? - an optional part that contains the default value in the format :DefaultValue.
    //  This part starts with : and captures all characters except },
    //  as } is the end of the replaceable substring.
    //
    let re = Regex::new(r"\$\{([a-zA-Z_][0-9a-zA-Z_]*)(?::([^}]*))?\}").unwrap();
    let mut err = Ok(());
    info!("Reading config file");
    info!("Expanding env variables");
    let result = re.replace_all(raw_config, |caps: &Captures| {
        let var = &caps[1];
        match env::var(var) {
            Ok(value) => {
                info!("Setting variable ${var}");
                value
            }
            Err(_) => match caps.get(2) {
                Some(default) if !default.as_str().is_empty() => {
                    info!("Variable ${var} not found. Getting from default");
                    default.as_str().to_string()
                }
                _ => {
                    error!("Variable ${var} not found. No default value found");
                    err = Err(anyhow::anyhow!("Variable ${var} not found"));
                    "".to_string()
                }
            },
        }
    });
    // not very beautiful, but works well enough that it doesn't bother me.
    err?;

    Ok(result.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn generate_test_var_name(base: &str) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        use std::time::{SystemTime, UNIX_EPOCH};

        let mut hasher = DefaultHasher::new();
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
            .hash(&mut hasher);
        let random_suffix = hasher.finish();

        format!("{}_{}", base, random_suffix)
    }

    #[test]
    fn test_expand_existing_env_var() {
        let test_var = generate_test_var_name("TEST_VAR");
        unsafe {
            std::env::set_var(&test_var, "test_value");
        }

        let input = format!("port: ${{{}}}", test_var);
        let result = expand_env_var(&input).unwrap();
        assert_eq!(result, "port: test_value");

        unsafe {
            std::env::remove_var(&test_var);
        }
    }

    #[test]
    fn test_expand_nonexistent_var_with_default() {
        let nonexistent_var = generate_test_var_name("NONEXISTENT_VAR");
        unsafe {
            std::env::remove_var(&nonexistent_var);
        }

        let input = format!("port: ${{{}:8080}}", nonexistent_var);
        let result = expand_env_var(&input).unwrap();
        assert_eq!(result, "port: 8080");
    }

    #[test]
    fn test_expand_nonexistent_var_without_default() {
        let nonexistent_var = generate_test_var_name("NONEXISTENT_VAR");
        unsafe {
            std::env::remove_var(&nonexistent_var);
        }

        let input = format!("port: ${{{}}}", nonexistent_var);
        let result = expand_env_var(&input);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[test]
    fn test_expand_multiple_vars() {
        let host_var = generate_test_var_name("HOST");
        let port_var = generate_test_var_name("PORT");
        unsafe {
            std::env::set_var(&host_var, "localhost");
        }
        unsafe {
            std::env::set_var(&port_var, "3000");
        }

        let input = format!(
            "endpoint:\n  host: ${{{}}}\n  port: ${{{}}}",
            host_var, port_var
        );
        let result = expand_env_var(&input).unwrap();
        assert_eq!(result, "endpoint:\n  host: localhost\n  port: 3000");

        unsafe {
            std::env::remove_var(&host_var);
        }
        unsafe {
            std::env::remove_var(&port_var);
        }
    }

    #[test]
    fn test_expand_mixed_existing_and_nonexisting() {
        let existing_var = generate_test_var_name("EXISTING_VAR");
        let missing_var = generate_test_var_name("MISSING_VAR");
        unsafe {
            std::env::set_var(&existing_var, "exists");
        }

        let input = format!(
            "existing: ${{{}}}\nnonexisting: ${{{}:default}}",
            existing_var, missing_var
        );
        let result = expand_env_var(&input).unwrap();
        assert_eq!(result, "existing: exists\nnonexisting: default");

        unsafe {
            std::env::remove_var(&existing_var);
        }
    }

    #[test]
    fn test_expand_empty_default() {
        let test_var = generate_test_var_name("TEST_VAR");
        unsafe {
            std::env::remove_var(&test_var);
        }

        let input = format!("value: ${{{}:}}", test_var);
        let result = expand_env_var(&input);
        assert!(result.is_err()); // Empty default should error
    }

    #[test]
    fn test_expand_no_vars() {
        let input = "port: 8080\nhost: localhost";
        let result = expand_env_var(input).unwrap();
        assert_eq!(result, input);
    }

    #[test]
    fn test_expand_var_with_special_chars() {
        let special_var = generate_test_var_name("SPECIAL_VAR");
        unsafe {
            std::env::set_var(&special_var, "value-with-dashes_and_123");
        }

        let input = format!("special: ${{{}}}", special_var);
        let result = expand_env_var(&input).unwrap();
        assert_eq!(result, "special: value-with-dashes_and_123");

        unsafe {
            std::env::remove_var(&special_var);
        }
    }

    #[test]
    fn test_expand_nested_vars_not_supported() {
        let outer_var = generate_test_var_name("OUTER");
        let inner_var = generate_test_var_name("INNER");
        unsafe {
            std::env::set_var(&outer_var, "${INNER}");
        }
        unsafe {
            std::env::set_var(&inner_var, "inner_value");
        }

        let input = format!("value: ${{{}}}", outer_var);
        let result = expand_env_var(&input).unwrap();
        // Should not expand nested - literal ${INNER}
        assert_eq!(result, "value: ${INNER}");

        unsafe {
            std::env::remove_var(&outer_var);
        }
        unsafe {
            std::env::remove_var(&inner_var);
        }
    }

    #[test]
    fn test_var_name_validation() {
        // Test valid variable names
        let valid_var = generate_test_var_name("VALID_VAR");
        unsafe {
            std::env::set_var(&valid_var, "value");
        }
        let result = expand_env_var(&format!("${{{}}}", valid_var)).unwrap();
        assert_eq!(result, "value");
        unsafe {
            std::env::remove_var(&valid_var);
        }

        // Test underscore start
        let underscore_var = generate_test_var_name("_UNDERSCORE");
        unsafe {
            std::env::set_var(&underscore_var, "value");
        }
        let result = expand_env_var(&format!("${{{}}}", underscore_var)).unwrap();
        assert_eq!(result, "value");
        unsafe {
            std::env::remove_var(&underscore_var);
        }

        // Test numbers in name
        let var123 = generate_test_var_name("VAR123");
        unsafe {
            std::env::set_var(&var123, "value");
        }
        let result = expand_env_var(&format!("${{{}}}", var123)).unwrap();
        assert_eq!(result, "value");
        unsafe {
            std::env::remove_var(&var123);
        }
    }
}
