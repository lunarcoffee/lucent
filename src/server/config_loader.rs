use serde::Deserialize;
use async_std::fs;

#[derive(Clone, Deserialize)]
pub struct Config {
    pub file_root: String,
    pub template_root: String,
    pub address: String,
    pub route_empty_to: String,
}

impl Config {
    pub async fn load(path: &str) -> Option<Self> {
        serde_yaml::from_str::<Config>(&fs::read_to_string(path).await.ok()?).ok()
    }
}
