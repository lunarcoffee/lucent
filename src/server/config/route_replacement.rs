use std::fmt::{self, Formatter};

use serde::{de::{self, Visitor}, Deserialize, Deserializer};

use crate::server::template::Template;

// A rule for URL rewriting, as specified by a `Template`; the rule syntax is identical to that of the templates. As
// with the templates, variables are allowed (they must be captured by the corresponding `RouteSpec`).
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
        formatter.write_str("string value starting with '/'")
    }

    fn visit_str<E>(self, str: &str) -> Result<Self::Value, E>
        where E: de::Error
    {
        // Make sure the rule starts with a slash (i.e. specifies a route). `Template::new` does syntax checking.
        str.starts_with('/').then(|| str)
            .and_then(|s| Template::new(s.to_string()))
            .map(|t| RouteReplacement(t))
            .ok_or(E::custom("expected route replacement"))
    }
}
