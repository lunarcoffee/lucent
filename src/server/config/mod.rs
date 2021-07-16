use std::collections::HashMap;

use async_std::fs;
use linked_hash_map::LinkedHashMap;
use serde::Deserialize;

use crate::server::config::auth_info::AuthInfo;
use crate::server::config::route_replacement::RouteReplacement;
use crate::server::config::route_spec::RouteSpec;

pub mod route_spec;
pub mod route_replacement;

pub mod auth_info;

#[derive(Clone, Deserialize)]
pub struct TlsConfig {
    pub cert_path: String,
    pub key_path: String,
}

#[derive(Clone, Deserialize)]
pub struct Config {
    pub file_root: String,
    pub template_root: String,
    pub address: String,
    pub cgi_executors: HashMap<String, String>,
    pub routing_table: LinkedHashMap<RouteSpec, RouteReplacement>,
    pub basic_auth: HashMap<RouteSpec, AuthInfo>,
    pub tls: Option<TlsConfig>,
}

impl Config {
    pub async fn load(path: &str) -> Option<Self> {
        serde_yaml::from_str::<Config>(&fs::read_to_string(path).await.ok()?).ok()
    }
}
