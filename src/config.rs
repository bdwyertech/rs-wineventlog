// Import the config crate's Config type and rename it to avoid confusion with our struct
use config::{Config as ConfigBuilder, Environment, File};
use serde::Deserialize;

// Our application's configuration structure
// The #[derive(Deserialize)] macro automatically generates code to convert
// data (from YAML, JSON, etc.) into this struct - similar to Go's struct tags
#[derive(Deserialize)]
pub struct Config {
    // Required field - must be present in config or will error
    pub channels: Vec<String>,

    // Optional field - if not present in config, defaults to None
    #[serde(default)]
    pub output_file: Option<String>,

    // Optional field with custom default function
    // If not present, calls default_batch_size() to get value
    #[serde(default = "default_batch_size")]
    pub batch_size: usize,
}

// Default value function for batch_size
// Called by serde when batch_size is missing from config
fn default_batch_size() -> usize {
    10
}

pub fn load(path: Option<String>) -> Result<Config, Box<dyn std::error::Error>> {
    // Determine config file path
    let config_path = match path {
        Some(p) => p,
        None => {
            // Default: look for config.yaml next to the executable
            let exe_dir = std::env::current_exe()?.parent().unwrap().to_path_buf();
            exe_dir.join("config.yaml").to_string_lossy().to_string()
        }
    };

    // Build configuration from multiple sources (similar to viper in Go)
    let settings = ConfigBuilder::builder()
        // Source 1: Load from YAML file
        // This reads config.yaml and parses it into a key-value map
        .add_source(File::with_name(&config_path))
        // Source 2: Load from environment variables
        // Looks for env vars like WINEVENTLOG_BATCH_SIZE, WINEVENTLOG_OUTPUT_FILE
        // The separator("_") means nested fields use underscores
        // Environment variables override file values (higher priority)
        .add_source(Environment::with_prefix("WINEVENTLOG").separator("_"))
        // Build the final merged configuration
        // This creates a config::Config (generic key-value map)
        .build()?;

    // Deserialize the generic config::Config into our specific Config struct
    // This is where serde's magic happens:
    // 1. Looks at our struct fields (output_file, channels, batch_size)
    // 2. Tries to find matching keys in the config map
    // 3. Converts types (string -> String, array -> Vec, etc.)
    // 4. Applies defaults for missing optional fields
    // 5. Returns error if required fields are missing
    Ok(settings.try_deserialize()?)
}
