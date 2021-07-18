use std::fmt::{self, Formatter};
use std::hash::{Hash, Hasher};

use regex::Regex;
use serde::{de, Deserialize, Deserializer};
use serde::de::Visitor;

// A rule which matches against routes. The syntax is just like a route, except you may capture parts of the route as
// variables (even conditionally, with regex). These can then be used in the corresponding `RouteReplacement`.
#[derive(Clone)]
pub struct RouteSpec(pub Regex);

// The following three impls allow `RouteSpec` to be used as a key in a hashmap.

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
        formatter.write_str("string value starting with '@/' or '/'")
    }

    fn visit_str<E>(self, str: &str) -> Result<Self::Value, E>
        where E: de::Error
    {
        // Rules starting with '@' only match exactly the route they specify, while rules starting with '/' match any
        // route which has a prefix matching the rule.
        match str.chars().next() {
            Some('@') => Ok(RouteSpec(convert_to_regex(&str[1..], true))),
            Some('/') => Ok(RouteSpec(convert_to_regex(str, false))),
            _ => Err(E::custom("expected route specifier")),
        }
    }
}

// this sucks lol TODO
fn convert_to_regex(route: &str, must_match_entire: bool) -> Regex {
    let chunked = isolate_var_captures(route);

    let (chunks, remainder_chunk) = chunked.as_chunks::<2>();
    let mut regex_str = "/".to_string();

    for [str, var] in chunks {
        regex_str.push_str(&regex::escape(&str[1..]));

        let mut split_var = var.splitn(2, ':');
        regex_str.push_str(&format!("(?P<{}>", &split_var.next().unwrap()[1..]));
        regex_str.push_str(&format!("{})", split_var.next().unwrap_or(".+")));
    }
    regex_str.push_str(&regex::escape(&remainder_chunk[0][1..]));

    regex_str = if must_match_entire { format!("^{}$", regex_str) } else { format!("^{}", regex_str) };
    Regex::new(&regex_str).unwrap()
}

fn isolate_var_captures(route: &str) -> Vec<String> {
    let mut is_var = false;
    let mut prev_is_escape = false;

    let partitioned = route.chars().filter_map(|c| {
        if !prev_is_escape {
            is_var = if c == '{' { true } else if c == '}' && is_var { false } else { is_var };
        }
        prev_is_escape = c == '\\';
        if prev_is_escape { None } else { Some((c, is_var)) }
    });

    let mut chunked = vec![];
    let mut prev_is_var = true;

    for (char, is_var) in partitioned {
        if is_var != prev_is_var {
            chunked.push(char.to_string());
            prev_is_var = is_var;
        } else {
            chunked.last_mut().unwrap().push(char);
        }
    }
    chunked
}
