use std::collections::HashMap;
use std::fmt::{Display, Formatter};
use std::fmt;

use crate::consts;
use crate::http::parser::{MessageParseError, MessageParseResult};
use crate::http::request::Method;
use crate::util;

pub struct Authority {
    pub user_info: Option<String>,
    pub host: String,
    pub port: Option<u16>,
}

impl Display for Authority {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let user_info = self.user_info.clone().map(|info| format!("{}@", info)).unwrap_or(String::new());
        let port = self.port.map(|port| format!(":{}", port)).unwrap_or(String::new());
        write!(f, "{}{}{}", encode_percent(&user_info), encode_percent(&self.host), port)
    }
}

pub struct AbsolutePath {
    pub path: Vec<String>,
    pub query: Option<HashMap<String, String>>,
}

impl AbsolutePath {
    pub fn path_as_string(&self) -> String {
        self.path.join("/")
    }

    pub fn query_as_string(&self) -> String {
        match &self.query {
            Some(query) => query
                .iter()
                .map(|(name, value)| format!("{}={}", name, value))
                .collect::<Vec<_>>()
                .join("&"),
            _ => String::new(),
        }
    }
}

impl Display for AbsolutePath {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let path_joined = self.path_as_string();
        let query_joined = self.query_as_string();
        write!(f, "/{}{}", encode_percent(&path_joined), encode_percent(&query_joined))
    }
}

pub enum Uri {
    OriginForm { path: AbsolutePath },
    AbsoluteForm {
        authority: Authority,
        path: AbsolutePath,
    },
    AuthorityForm { authority: Authority },
    AsteriskForm,
}

impl Uri {
    pub fn from(method: &Method, raw: &str) -> MessageParseResult<Self> {
        UriParser { method, raw }.parse()
    }
}

impl Display for Uri {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Uri::OriginForm { path } => write!(f, "{}", path),
            Uri::AbsoluteForm { authority, path } => write!(f, "http://{}{}", authority, path),
            Uri::AuthorityForm { authority } => write!(f, "{}", authority),
            Uri::AsteriskForm => write!(f, "*"),
        }
    }
}

struct UriParser<'a, 'b> {
    method: &'a Method,
    raw: &'b str,
}

impl UriParser<'_, '_> {
    fn parse(&mut self) -> MessageParseResult<Uri> {
        if self.raw.len() > consts::MAX_URI_LENGTH {
            Err(MessageParseError::UriTooLong)
        } else if self.raw == "*" && *self.method == Method::Options {
            Ok(Uri::AsteriskForm)
        } else if *self.method == Method::Connect {
            let authority = self.parse_authority(false)?;
            Ok(Uri::AuthorityForm { authority })
        } else if self.raw.starts_with('/') {
            let path = self.parse_absolute_path(false)?;
            Ok(Uri::OriginForm { path })
        } else {
            self.parse_pre_authority()?;
            let authority = self.parse_authority(true)?;
            let path = self.parse_absolute_path(true)?;
            Ok(Uri::AbsoluteForm { authority, path })
        }
    }

    fn parse_pre_authority(&mut self) -> MessageParseResult<()> {
        if self.raw.starts_with("http://") && self.raw.len() > 7 {
            self.raw = &self.raw[7..];
        } else if self.raw.starts_with("https://") && self.raw.len() > 8 {
            self.raw = &self.raw[8..];
        } else {
            return Self::uri_error();
        }
        Ok(())
    }

    fn parse_authority(&mut self, accept_user: bool) -> MessageParseResult<Authority> {
        let authority_part = &self.raw[..self.raw.find('/').unwrap_or(self.raw.len())];
        let user_info = if let Some(index) = authority_part.find('@') {
            let info = &authority_part[..index];
            if accept_user && info.chars().all(|c| is_user_info_char(c)) {
                Some(decode_percent(info).ok_or(MessageParseError::InvalidUri)?)
            } else {
                return Self::uri_error();
            }
        } else {
            None
        };

        let host_and_port = authority_part.split(':').collect::<Vec<_>>();
        if host_and_port.is_empty() || host_and_port.len() > 2 {
            return Self::uri_error();
        }

        let mut host = host_and_port[0].to_string();
        if host.chars().any(|c| !is_host_char(c)) {
            return Self::uri_error();
        }
        host = decode_percent(&host).ok_or(MessageParseError::InvalidUri)?;

        let port = match host_and_port.get(1).map(|s| s.parse()) {
            Some(Ok(port)) => Some(port),
            Some(Err(_)) => return Self::uri_error(),
            _ => None,
        };

        self.raw = &self.raw[authority_part.len()..];
        Ok(Authority { user_info, host, port })
    }

    fn parse_absolute_path(&mut self, accept_empty: bool) -> MessageParseResult<AbsolutePath> {
        if !accept_empty && (self.raw.is_empty() || !self.raw.starts_with('/')) {
            return Self::uri_error();
        }

        let (mut raw_path, raw_query) = if let Some(index) = self.raw.find('?') {
            (&self.raw[..index], &self.raw[index + 1..])
        } else {
            (self.raw, "")
        };

        if raw_path.ends_with('/') {
            raw_path = &raw_path[..raw_path.len() - 1]
        }

        let mut path = raw_path.split('/').map(|segment| segment.to_string()).collect::<Vec<_>>();
        if !path[0].is_empty() {
            return Self::uri_error();
        }
        path = path.into_iter().skip(1).map(|s| s.to_string()).collect();
        if path.iter().any(|part| part.is_empty() || part.chars().any(|c| !is_path_char(c)) || part == "..") {
            return Self::uri_error();
        }
        let old_len = path.len();
        path = path.iter().filter_map(|s| decode_percent(s)).collect();
        if path.len() < old_len {
            return Self::uri_error();
        }

        if raw_query.is_empty() {
            Ok(AbsolutePath { path, query: None })
        } else {
            let params = raw_query
                .split('&')
                .map(|param| param.splitn(2, '=').collect::<Vec<&str>>())
                .collect::<Vec<_>>();

            if params.iter().all(|p| p.len() == 2 && is_query_string(p[0]) && is_query_string(p[1])) {
                let query = params
                    .iter()
                    .filter_map(|p| Some((decode_percent(p[0])?, decode_percent(p[1])?)))
                    .collect::<HashMap<_, _>>();
                if query.len() < params.len() {
                    return Self::uri_error();
                }
                Ok(AbsolutePath { path, query: Some(query) })
            } else {
                Self::uri_error()
            }
        }
    }

    const fn uri_error<T>() -> MessageParseResult<T> {
        Err(MessageParseError::InvalidUri)
    }
}

fn is_query_string(str: &str) -> bool {
    str.chars().all(is_query_char)
}

fn is_query_char(ch: char) -> bool {
    is_path_char(ch) || ch == '/' || ch == '?'
}

fn is_path_char(ch: char) -> bool {
    is_user_info_char(ch) || ch == '@'
}

fn is_user_info_char(ch: char) -> bool {
    is_host_char(ch) || ch == ':'
}

const HOST_CHARS: &str = "-._~%!$&'()*+,;=";

fn is_host_char(ch: char) -> bool {
    HOST_CHARS.contains(ch) || ch.is_ascii_alphanumeric()
}

fn decode_percent(str: &str) -> Option<String> {
    let mut decoded = String::new();
    let mut last_index = 0;
    for (index, _) in str.match_indices('%') {
        decoded.push_str(&str[last_index..index]);
        if index + 3 > str.len() {
            return None;
        }
        let ch = u8::from_str_radix(&str[index + 1..index + 3], 16).ok()? as char;
        decoded.push(ch);
        last_index = index + 3;
    }
    decoded.push_str(&str[last_index..]);
    Some(decoded)
}

fn encode_percent(str: &str) -> String {
    str.chars()
        .map(|c| if util::is_visible_char(c) { c.to_string() } else { format!("%{:02x}", c as u8) })
        .collect::<Vec<_>>()
        .join("")
}
