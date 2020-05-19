use serde::{de, Deserialize, Deserializer};
use serde::de::Visitor;
use serde::export::{fmt, Formatter};

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
        .map(|(s, is_var)| (s.splitn(2, ':').collect::<Vec<_>>(), is_var))
        .map(|(s, is_var)| if *is_var {
            match s.len() {
                1 => format!("(?P<{}>.+)", &s[0][1..s[0].len() - 1]),
                _ => format!("(?P<{}>{})", &s[0][1..], s.get(1).map(|s| &s[..s.len() - 1]).unwrap_or(&".+")),
            }
        } else {
            regex::escape(s[0])
        })
        .collect::<String>();

    regex_str = if must_match_entire { format!("^{}$", regex_str) } else { format!("^{}", regex_str) };
    Regex::new(&regex_str).unwrap()
}
