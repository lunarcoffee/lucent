use std::collections::HashMap;

use async_std::fs;
use linked_hash_map::LinkedHashMap;
use serde::{de, Deserialize, Deserializer};
use serde::de::Visitor;
use serde::export::{fmt, Formatter};

use crate::server::template::Template;
use regex::Regex;
use std::hash::{Hash, Hasher};

#[derive(Clone)]
pub struct RouteSpec(pub Regex);

impl Hash for RouteSpec {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write(self.0.as_str().as_bytes());
    }
}

impl Eq for RouteSpec {}

impl PartialEq for RouteSpec {
    fn eq(&self, other: &Self) -> bool {
        self.0.as_str() == other.0.as_str()
    }
}

impl<'a> Deserialize<'a> for RouteSpec {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where D: Deserializer<'a>
    {
        deserializer.deserialize_str(RouteSpecStringVisitor)
    }
}

impl<'a> Deserialize<'a> for Template {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where D: Deserializer<'a>
    {
        deserializer.deserialize_str(TemplateStringVisitor)
    }
}

struct RouteSpecStringVisitor;

impl<'a> Visitor<'a> for RouteSpecStringVisitor {
    type Value = RouteSpec;

    fn expecting(&self, formatter: &mut Formatter) -> fmt::Result {
        formatter.write_str("String value beginning with `@/` or `/`.")
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
        where E: de::Error
    {
        let err = E::custom(format!("Route specifier invalid: {}", value));
        match value.chars().next() {
            Some('@') => Ok(RouteSpec(convert_to_regex(&value[1..], true))),
            Some('/') => Ok(RouteSpec(convert_to_regex(value, false))),
            _ => Err(err),
        }
    }
}

struct TemplateStringVisitor;

impl<'a> Visitor<'a> for TemplateStringVisitor {
    type Value = Template;

    fn expecting(&self, formatter: &mut Formatter) -> fmt::Result {
        formatter.write_str("String value beginning with `/`.")
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
        where E: de::Error
    {
        let err = E::custom(format!("Route replacement invalid: {}", value));
        match value.chars().next() {
            Some('/') => Ok(Template::new(value.to_string()).ok_or(err)?),
            _ => Err(err),
        }
    }
}

#[derive(Clone, Deserialize)]
pub struct Config {
    pub file_root: String,
    pub template_root: String,
    pub address: String,
    pub cgi_executors: HashMap<String, String>,
    pub routing_table: LinkedHashMap<RouteSpec, Template>,
}

impl Config {
    pub async fn load(path: &str) -> Option<Self> {
        serde_yaml::from_str::<Config>(&fs::read_to_string(path).await.ok()?).ok()
    }
}

fn convert_to_regex(route: &str, must_match_entire: bool) -> Regex {
    let mut is_var = false;
    let partitioned = route.chars().map(|c| {
        is_var = if c == '{' { true } else if c == '}' && is_var { false } else { is_var };
        (c, is_var || c == '}')
    });
    let chunked = partitioned.fold(Vec::<(String, bool)>::new(), |mut acc, (c, is_var)| {
        if is_var != acc.last().map(|c| c.1).unwrap_or(!is_var) {
            acc.push((c.to_string(), is_var));
        } else {
            *acc.last_mut().unwrap() = acc.last().map(|(s, is_var)| (format!("{}{}", s, c), *is_var)).unwrap();
        }
        acc
    });

    let mut regex_str = chunked
        .iter()
        .map(|(s, is_var)| if *is_var { format!("(?P<{}>.+)", &s[1..s.len() - 1]) } else { regex::escape(s) })
        .collect::<String>();
    regex_str = if must_match_entire { format!("^{}$", regex_str) } else { format!("^{}", regex_str) };
    Regex::new(&regex_str).unwrap()
}
