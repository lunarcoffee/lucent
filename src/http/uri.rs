use std::collections::HashMap;
use std::fmt::{Display, Formatter};
use std::fmt;

use crate::http::consts;
use crate::http::request::{Method, RequestParseError, RequestParseResult};

pub struct Authority {
    user_info: Option<String>,
    host: String,
    port: Option<u16>,
}

impl Display for Authority {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let user_info = self.user_info.clone().map(|info| format!("{}@", info)).unwrap_or(String::new());
        let port = self.port.map(|port| format!(":{}", port)).unwrap_or(String::new());
        write!(f, "{}{}{}", user_info, self.host, port)
    }
}

pub struct AbsolutePath {
    path: Vec<String>,
    query: Option<HashMap<String, String>>,
}

impl Display for AbsolutePath {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let path_joined = self.path.join("/");
        let query_joined = match &self.query {
            Some(query) => {
                let joined = query
                    .iter()
                    .map(|(name, value)| format!("{}={}", name, value))
                    .collect::<Vec<_>>()
                    .join("&");
                format!("?{}", joined)
            }
            _ => String::new(),
        };
        write!(f, "/{}{}", path_joined, query_joined)
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
    pub fn from(method: &Method, raw: &str) -> RequestParseResult<Self> {
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

struct UriParser<'a> {
    method: &'a Method,
    raw: &'a str,
}

impl UriParser<'_> {
    fn parse(&mut self) -> RequestParseResult<Uri> {
        if self.raw.len() > consts::MAX_URI_LENGTH {
            Err(RequestParseError::UriTooLong)
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

    fn parse_pre_authority(&mut self) -> RequestParseResult<()> {
        if self.raw.starts_with("http://") && self.raw.len() > 7 {
            self.raw = &self.raw[7..];
        } else if self.raw.starts_with("https://") && self.raw.len() > 8 {
            self.raw = &self.raw[8..];
        } else {
            return Err(RequestParseError::InvalidUri);
        }
        Ok(())
    }

    fn parse_authority(&mut self, accept_user: bool) -> RequestParseResult<Authority> {
        let authority_part = &self.raw[..self.raw.find('/').unwrap_or(self.raw.len())];
        let user_info = if let Some(index) = authority_part.find('@') {
            let info = &authority_part[..index];
            if accept_user && info.chars().all(|c| is_user_info_char(c)) {
                Some(info.to_string())
            } else {
                return Err(RequestParseError::InvalidUri);
            }
        } else {
            None
        };

        let host_and_port = authority_part.split(':').collect::<Vec<_>>();
        if host_and_port.is_empty() || host_and_port.len() > 2 {
            return Err(RequestParseError::InvalidUri);
        }

        let host = host_and_port[0].to_string();
        if host.chars().any(|c| !is_host_char(c)) {
            return Err(RequestParseError::InvalidUri);
        }

        let port = match host_and_port.get(1).map(|s| s.parse()) {
            Some(Ok(port)) => Some(port),
            Some(Err(_)) => return Err(RequestParseError::InvalidUri),
            _ => None,
        };

        self.raw = &self.raw[authority_part.len()..];
        Ok(Authority { user_info, host, port })
    }

    fn parse_absolute_path(&mut self, accept_empty: bool) -> RequestParseResult<AbsolutePath> {
        if !accept_empty && (self.raw.is_empty() || !self.raw.starts_with('/')) {
            return Err(RequestParseError::InvalidUri);
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
            return Err(RequestParseError::InvalidUri);
        }
        path = path.into_iter().skip(1).map(|s| s.to_string()).collect();
        if path.iter().any(|part| part.is_empty() || part.chars().any(|c| !is_path_char(c))) {
            return Err(RequestParseError::InvalidUri);
        }

        if raw_query.is_empty() {
            Ok(AbsolutePath { path, query: None })
        } else {
            let params = raw_query
                .split('&')
                .map(|param| param.splitn(2, '=').collect::<Vec<&str>>())
                .collect::<Vec<_>>();

            if params.iter().all(|p| p.len() == 2 && is_query_string(p[0]) && is_query_string(p[1])) {
                let query = params.iter().map(|p| (p[0].to_string(), p[1].to_string())).collect();
                Ok(AbsolutePath { path, query: Some(query) })
            } else {
                Err(RequestParseError::InvalidUri)
            }
        }
    }
}

fn is_query_string(str: &str) -> bool {
    str.chars().all(|c| is_query_char(c))
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
