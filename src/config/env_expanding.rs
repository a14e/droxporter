use std::env;
use regex::{Captures, Regex};
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
    let result = re.replace_all(&raw_config, |caps: &Captures| {
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
