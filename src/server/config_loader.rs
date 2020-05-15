use serde::{Deserialize, Deserializer, de};
use async_std::fs;
use std::collections::HashMap;
use serde::de::Visitor;
use serde::export::{Formatter, fmt};
use linked_hash_map::LinkedHashMap;

#[derive(Clone, Eq, Hash, PartialEq)]
pub enum RouteSpec {
    StartsWith(String),
    Matches(String),
}

struct RouteSpecStringVisitor;

impl<'a> Visitor<'a> for RouteSpecStringVisitor {
    type Value = RouteSpec;

    fn expecting(&self, formatter: &mut Formatter) -> fmt::Result {
        formatter.write_str("String value beginning with `@` or `/`.")
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
        where E: de::Error
    {
        if value.starts_with('@') {
            Ok(RouteSpec::Matches(value[1..].to_string()))
        } else if value.starts_with('/') {
            Ok(RouteSpec::StartsWith(value.to_string()))
        } else {
            Err(E::custom(format!("Route specifier does not start with `@` or `/`: {}", value)))
        }
    }
}

impl<'a> Deserialize<'a> for RouteSpec {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where D: Deserializer<'a>
    {
        deserializer.deserialize_str(RouteSpecStringVisitor)
    }
}

#[derive(Clone, Deserialize)]
pub struct Config {
    pub file_root: String,
    pub template_root: String,
    pub address: String,
    pub cgi_executors: HashMap<String, String>,
    pub routing_table: LinkedHashMap<RouteSpec, String>,
}

impl Config {
    pub async fn load(path: &str) -> Option<Self> {
        serde_yaml::from_str::<Config>(&fs::read_to_string(path).await.ok()?).ok()
    }
}
