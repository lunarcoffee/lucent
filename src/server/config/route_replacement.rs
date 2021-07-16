use std::fmt::{self, Formatter};

use serde::{de, Deserialize, Deserializer};
use serde::de::Visitor;

use crate::server::template::Template;

#[derive(Clone)]
pub struct RouteReplacement(pub Template);

impl<'a> Deserialize<'a> for RouteReplacement {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where D: Deserializer<'a>
    {
        deserializer.deserialize_str(RouteReplacementStringVisitor)
    }
}

struct RouteReplacementStringVisitor;

impl<'a> Visitor<'a> for RouteReplacementStringVisitor {
    type Value = RouteReplacement;

    fn expecting(&self, formatter: &mut Formatter) -> fmt::Result {
        formatter.write_str("String value beginning with `/`.")
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
        where E: de::Error
    {
        let err = E::custom(format!("Route replacement invalid: {}", value));
        match value.chars().next() {
            Some('/') => Ok(RouteReplacement(Template::new(value.to_string()).ok_or(err)?)),
            _ => Err(err),
        }
    }
}
