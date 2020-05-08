use std::collections::HashMap;
use std::fmt::{Debug, Formatter};
use std::fmt;

use crate::http::consts;

const MULTI_VALUE_HEADER_NAMES: &[&str] = &[
    consts::H_ACCEPT, consts::H_ACCEPT_CHARSET, consts::H_ACCEPT_ENCODING, consts::H_ACCEPT_LANGUAGE,
    consts::H_CACHE_CONTROL, consts::H_TE, consts::H_TRANSFER_ENCODING, consts::H_UPGRADE, consts::H_VIA,
];

pub struct Headers {
    headers: HashMap<String, Vec<String>>,
}

impl Headers {
    pub fn from(headers: HashMap<String, Vec<String>>) -> Self {
        Headers { headers }
    }

    pub fn get(&self, name: &str) -> Option<&Vec<String>> {
        self.headers.get(&Self::normalize_header_name(name))
    }

    pub fn set(&mut self, name: &str, mut value: Vec<String>) -> bool {
        if !is_valid_header_name(name) || value.iter().any(|v| !is_valid_header_value(v)) {
            false
        } else {
            self.headers.entry(Self::normalize_header_name(name)).or_insert(vec![]).append(&mut value);
            true
        }
    }

    pub fn is_multi_value(name: &str) -> bool {
        MULTI_VALUE_HEADER_NAMES.contains(&&*Self::normalize_header_name(name))
    }

    fn normalize_header_name(name: &str) -> String {
        name.to_ascii_lowercase()
    }
}

impl Debug for Headers {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let headers_joined = self
            .headers
            .iter()
            .map(|h| format!("{}: {}", h.0, h.1.join(", ")))
            .collect::<Vec<String>>()
            .join("\n");
        write!(f, "{}", headers_joined)
    }
}

fn is_valid_header_name(str: &str) -> bool {
    str.chars().all(|c| is_token_char(c))
}

fn is_valid_header_value(str: &str) -> bool {
    str.chars().all(|c| is_visible_char(c) || consts::OPTIONAL_WHITESPACE.contains(&c))
}

const TOKEN_CHARS: &str = "!#$%&'*+-.^_`|~";

fn is_token_char(ch: char) -> bool {
    TOKEN_CHARS.contains(ch) || ch.is_ascii_alphanumeric()
}

fn is_visible_char(ch: char) -> bool {
    ('!'..='~').contains(&ch)
}
