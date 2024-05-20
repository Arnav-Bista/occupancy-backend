use serde::Deserialize;

#[derive(Deserialize)]
pub struct Config {
    pub url: String,
    pub headers: String,
    pub scrape_regex: String,
}

impl Config {
    pub fn from_config(config: String) -> Result<Self, String> {
        match serde_json::from_str(&config) {
            Ok(data) => Ok(data),
            Err(err) => Err(format!("Could not deserialize.\n{}",err.to_string()).to_owned()),
        }
    }
}
