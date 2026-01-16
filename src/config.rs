use serde::Deserialize;
use std::fs;

#[derive(Deserialize)]
pub struct Config {
    pub output_file: Option<String>,
    pub channels: Vec<String>,
}

pub fn load(path: Option<String>) -> Result<Config, Box<dyn std::error::Error>> {
    let config_path = match path {
        Some(p) => p.into(),
        None => {
            let exe_dir = std::env::current_exe()?.parent().unwrap().to_path_buf();
            exe_dir.join("config.yaml")
        }
    };

    let content = fs::read_to_string(config_path)?;
    Ok(serde_yaml::from_str(&content)?)
}
