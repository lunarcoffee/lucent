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

// Converts the raw route specifier (`route`) into the corresponding regex. If `must_match_entire` is true, the regex
// will only match the route given exactly. Otherwise, it will match any route with a matching prefix.
fn convert_to_regex(route: &str, match_exact: bool) -> Regex {
    let isolated = isolate_var_captures(route);

    // Since the string and variable captures are alternating, we process them in chunks of two. The first will always
    // be a string, and the second will always be a variable capture.
    let (chunks, remainder_chunk) = isolated.as_chunks::<2>();
    let mut regex_str = "/".to_string();

    // Build up `regex_str` by processing each adjacent pair of chunks.
    for [str, var] in chunks {
        // Slicing off the first character removes the leftover '}'; see the comment above `isolate_var_captures`.
        regex_str.push_str(&regex::escape(&str[1..]));

        // Append the variable capture and the regex, if present. If no regex is present, accept any non-empty string
        // of characters ('.+'). Also note the slicing for the capture, which removes the leading '{'.
        let mut split_var = var.splitn(2, ':');
        regex_str.push_str(&format!("(?P<{}>", &split_var.next().unwrap()[1..]));
        regex_str.push_str(&format!("{})", split_var.next().unwrap_or(".+")));
    }

    // Append the remaining string chunk, if present; note the same slicing mentioned earlier.
    if !remainder_chunk.is_empty() {
        regex_str.push_str(&regex::escape(&remainder_chunk[0][1..]));
    }

    // Account for whether the regex must match a route exactly.
    regex_str = if match_exact { format!("^{}$", regex_str) } else { format!("^{}", regex_str) };
    Regex::new(&regex_str).unwrap()
}

// Turns a raw route specifier into a list alternating between strings and variable captures. For example, '/a/{var}/b'
// would become ['/a/', '{var', '}/b']; note that the '}' is not considered part of the variable capture in this
// context, but it is still required to be present.
fn isolate_var_captures(route: &str) -> Vec<String> {
    // `is_var` represents whether the current character belongs to a capture, and `prev_is_escape` represents whether
    // the previous character is the escape character ('\').
    let mut is_var = false;
    let mut prev_is_escape = false;

    // Map each character of the string to a boolean representing whether it is in a capture or not.
    let mapped = route.chars().filter_map(|c| {
        // If the previous character was '\', don't change anything; the current character is special. Otherwise,
        // this maintains the invariant of `is_var` as specified in the comment earlier, with the exception that the
        // terminating '}' is not considered part of a capture.
        if !prev_is_escape {
            is_var = if c == '{' { true } else if c == '}' && is_var { false } else { is_var };
        }
        prev_is_escape = c == '\\';

        // Don't include escape characters in the final output.
        if prev_is_escape { None } else { Some((c, is_var)) }
    });

    // Combine adjacent characters into strings, based on whether they are in a capture or not.
    mapped.collect::<Vec<_>>()
        .group_by(|a, b| a.1 == b.1)
        .map(|g| g.into_iter().map(|(c, _)| c).collect())
        .collect()
}
