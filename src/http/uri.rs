use std::collections::HashMap;
use std::fmt::{Display, Formatter};
use std::fmt;

use crate::consts;
use crate::http::parser::{MessageParseError, MessageParseResult};
use crate::http::request::Method;
use crate::util;

// The authority portion of a URI containing the host, and some optional info.
pub struct Authority {
    // This includes the username and/or password.
    pub user_info: Option<String>,

    pub host: String,
    pub port: Option<u16>,
}

// All `Display` implementations format the URI component it is implemented on as laid out in the spec.
impl Display for Authority {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let user_info = self.user_info.clone().map(|info| format!("{}@", info)).unwrap_or(String::new());
        let port = self.port.map(|port| format!(":{}", port)).unwrap_or(String::new());
        write!(f, "{}{}{}", encode_percent(&user_info), encode_percent(&self.host), port)
    }
}

// A parsed query string.
pub enum Query {
    ParamMap(HashMap<String, String>),

    // Used with GET or HEAD requests handled by CGI scripts (see section 4.4 in RFC 3875).
    SearchString(Vec<String>),
    None,
}

// An absolute path with optional query parameters.
pub struct AbsolutePath {
    pub path: Vec<String>,
    pub query: Query,
}

impl AbsolutePath {
    pub fn path_as_string(&self) -> String {
        self.path.join("/")
    }

    // Returns the formatted query string.
    pub fn query_as_string(&self) -> String {
        match &self.query {
            Query::ParamMap(map) => map
                .iter()
                .map(|(name, value)| format!("{}={}", name, value))
                .collect::<Vec<_>>()
                .join("&"),
            Query::SearchString(terms) => terms.join("+"),
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

// A URI, in one of the four forms specified in RFC 7230 for use in HTTP requests.
pub enum Uri {
    // Origin-form specifies only a path (i.e. 'GET /index.html'), with the host specified in the 'Host' header.
    OriginForm { path: AbsolutePath },

    // Absolute-form specifies both host and path.
    AbsoluteForm {
        scheme: String,
        authority: Authority,
        path: AbsolutePath,
    },

    // Authority-form specifies only the authority, used only for CONNECT requests.
    AuthorityForm { authority: Authority },

    // Asterisk-form ('*') is used only for a server-wide OPTIONS request.
    AsteriskForm,
}

impl Uri {
    // Attempts to parse a URI from `raw`, validating method-specific restrictions (i.e. asterisk-form '*' is only for
    // CONNECT requests) with the given `method`.
    pub fn from(method: &Method, raw: &str) -> MessageParseResult<Self> {
        UriParser { method, raw }.parse()
    }

    pub fn query(&self) -> &Query {
        match self {
            Uri::OriginForm { path, .. } => &path.query,
            Uri::AbsoluteForm { path, .. } => &path.query,
            _ => &Query::None,
        }
    }

    pub fn to_string_no_query(&self) -> String {
        match self {
            Uri::OriginForm { path } => format!("{}", path.path_as_string()),
            Uri::AbsoluteForm { scheme, authority, path } =>
                format!("{}://{}{}", scheme, authority, path.path_as_string()),
            _ => format!("{}", self),
        }
    }
}

// Formats the URI in accordance to the forms specified in RFC 7230.
impl Display for Uri {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Uri::OriginForm { path } => write!(f, "{}", path),
            Uri::AbsoluteForm { scheme, authority, path } => write!(f, "{}://{}{}", scheme, authority, path),
            Uri::AuthorityForm { authority } => write!(f, "{}", authority),
            _ => write!(f, "*"),
        }
    }
}

// Simplifies a common pattern where a condition requires the parser to terminate with an error, which often translates
// to returning from a function.
macro_rules! err_if {
    ($cond:expr) => {
        if $cond {
            return Err(MessageParseError::InvalidUri);
        }
    }
}

// This is a parser for a URI, parsing `raw` as if it came from an HTTP request with the given `method`.
struct UriParser<'a, 'b> {
    method: &'a Method,
    raw: &'b str,
}

impl UriParser<'_, '_> {
    fn parse(&mut self) -> MessageParseResult<Uri> {
        if self.raw.len() > consts::MAX_URI_LENGTH {
            Err(MessageParseError::UriTooLong)
        } else if self.raw == "*" && *self.method == Method::Options {
            // Asterisk-form is only used in options requests.
            Ok(Uri::AsteriskForm)
        } else if *self.method == Method::Connect {
            // Connect requests should always use authority-form.
            let authority = self.parse_authority(false)?;
            Ok(Uri::AuthorityForm { authority })
        } else if self.raw.starts_with('/') {
            // Targets starting with '/' should be in origin-form.
            let path = self.parse_absolute_path(false)?;
            Ok(Uri::OriginForm { path })
        } else {
            // Otherwise, assume authority-form.
            let scheme = self.parse_pre_authority()?;
            let authority = self.parse_authority(true)?;
            let path = self.parse_absolute_path(true)?;
            Ok(Uri::AbsoluteForm { scheme, authority, path })
        }
    }

    // Attempts to parse the URI scheme (only HTTP and HTTPS are supported).
    fn parse_pre_authority(&mut self) -> MessageParseResult<String> {
        let scheme = if self.raw.starts_with("http://") && self.raw.len() > 7 {
            // Prevent the next stage in parsing from parsing the scheme again.
            self.raw = &self.raw[7..];
            "http"
        } else if self.raw.starts_with("https://") && self.raw.len() > 8 {
            self.raw = &self.raw[8..];
            "https"
        } else {
            // Only HTTP and HTTPS schemes are supported.
            return Err(MessageParseError::InvalidUri);
        };
        Ok(scheme.to_string())
    }

    fn parse_authority(&mut self, accept_user: bool) -> MessageParseResult<Authority> {
        // Take the substring until the first '/' as the authority, using the whole string if no '/' is present. The
        // '/' represents the beginning of the target resource's path.
        let mut authority_part = &self.raw[..self.raw.find('/').unwrap_or(self.raw.len())];

        // If there exists an '@', it must be preceded by user info.
        let user_info = match authority_part.find('@') {
            Some(index) => {
                // Terminate if invalid characters are found.
                let info = &authority_part[..index].to_string();
                err_if!(!accept_user || !info.chars().all(is_user_info_char));

                // Prevent the user info part from being parsed again.
                authority_part = &authority_part[index + 1..];

                // Percent-decode the information. We don't need to use the username or password separately, so we
                // don't bother parsing them separately.
                Some(decode_percent(info).ok_or(MessageParseError::InvalidUri)?)
            }
            _ => None,
        };

        // Split the remaining authority part into the host and port (if present). There should not be more than two
        // parts (the host and port), and neither should be empty.
        let host_and_port = authority_part.split(':').collect::<Vec<_>>();
        err_if!(host_and_port.is_empty() || host_and_port.len() > 2);

        let mut host = host_and_port[0].to_string();
        err_if!(!host.chars().all(is_host_char));
        host = decode_percent(&host).ok_or(MessageParseError::InvalidUri)?;

        // Parse the port. This `parse` call infers that it should be a u16 based on its use in the struct literal
        // later, so that bit of validation is done here.
        let port = match host_and_port.get(1).map(|s| s.parse()) {
            Some(Ok(port)) => Some(port),
            Some(Err(_)) => return Err(MessageParseError::InvalidUri),
            _ => None,
        };

        // Prevent the next parsing stage from parsing this again.
        self.raw = &self.raw[authority_part.len()..];
        Ok(Authority { user_info, host, port })
    }

    // Attempts to parse an absolute path. `accept_empty` determines if an empty path is allowed.
    fn parse_absolute_path(&mut self, accept_empty: bool) -> MessageParseResult<AbsolutePath> {
        // Make sure the path part isn't empty, or that an empty path is allowed. Also, a non-'/' prefix is valid if
        // query parameters are specified (with an empty path, that would start with a '?').
        err_if!(!accept_empty && (self.raw.is_empty() || !self.raw.starts_with('/')));

        // Split this part into the path and query.
        let (mut raw_path, raw_query) = if let Some(index) = self.raw.find('?') {
            (&self.raw[..index], &self.raw[index + 1..])
        } else {
            (self.raw, "")
        };

        // Trim a singular trailing '/', if present.
        if raw_path.ends_with('/') {
            raw_path = &raw_path[..raw_path.len() - 1]
        }

        // Split the path into its segments. Terminate if the path started with something before the first '/' (i.e.
        // 'hello/world'; '/hello/world' is valid).
        let mut path = raw_path.split('/').map(|segment| segment.to_string()).collect::<Vec<_>>();
        err_if!(!path[0].is_empty());

        // Remove the empty segment and check for invalid characters.
        path.remove(0);
        err_if!(path.iter().any(|part| part.is_empty() || !part.chars().all(is_path_char) || part == ".."));

        // Percent-decode each segment.
        for segment in path.iter_mut() {
            *segment = decode_percent(&segment).ok_or(MessageParseError::InvalidUri)?;
        }

        // Parse the query.
        Ok(AbsolutePath { path, query: parse_query(raw_query)? })
    }
}

// Parse the query parameters, if present (non-empty).
fn parse_query(raw_query: &str) -> MessageParseResult<Query> {
    Ok(if raw_query.is_empty() {
        Query::None
    } else if raw_query.contains('=') {
        // Split each query parameter pair, then split each pair into key and value.
        let params = raw_query.split('&')
            .map(|param| param.splitn(2, '=').collect::<Vec<&str>>())
            .collect::<Vec<_>>();

        // Terminate if not all parameter pairs are of length two (i.e. if there was no '=' to split on), or if
        // there are invalid characters anywhere.
        err_if!(!params.iter()
                .all(|p| p.len() == 2 && p[0].chars().all(is_query_char) && p[1].chars().all(is_query_char)));

        // Percent-decode the parameters.
        let query = params.iter()
            .map(|p| Some((decode_percent(p[0])?, decode_percent(p[1])?)))
            .collect::<Option<HashMap<_, _>>>()
            .ok_or(MessageParseError::InvalidUri)?;
        Query::ParamMap(query)
    } else {
        // Split into pieces and decode.
        let params = raw_query.split('+')
            .map(|term| decode_percent(term))
            .collect::<Option<Vec<_>>>()
            .ok_or(MessageParseError::InvalidUri)?;
        Query::SearchString(params)
    })
}

// The URI spec defines many sets of characters, using them to specify which characters are allowed in what parts of a
// URI. These functions are predicates that determine if a character is in one of those sets (i.e. if it is allowed in
// a certain part of a URI).

fn is_query_char(ch: char) -> bool {
    is_path_char(ch) || ch == '/' || ch == '?'
}

fn is_path_char(ch: char) -> bool {
    is_user_info_char(ch) || ch == '@'
}

fn is_user_info_char(ch: char) -> bool {
    is_host_char(ch) || ch == ':'
}

fn is_host_char(ch: char) -> bool {
    const HOST_CHARS: &str = "-._~%!$&'()*+,;=";
    HOST_CHARS.contains(ch) || ch.is_ascii_alphanumeric()
}

// Attempts to decode the given percent-encoded string.
fn decode_percent(str: &str) -> Option<String> {
    let mut decoded = String::new();

    // The index after the end of the previous encoded character (i.e. the index marked by the caret in '%AE^').
    let mut last_index = 0;

    // Go through every index of a '%' in the string, attempting to decode each one.
    for (index, _) in str.match_indices('%') {
        // Append the substring between the end of the previous encoded character and the start of the current one.
        decoded.push_str(&str[last_index..index]);

        // If there are less than two characters after a '%', the string is invalid.
        if index + 3 > str.len() {
            return None;
        }

        // Decode and append.
        let ch = u8::from_str_radix(&str[index + 1..index + 3], 16).ok()? as char;
        decoded.push(ch);

        // `index` is the index of the '%' character, so `index + 3` is after the end of the encoded character.
        last_index = index + 3;
    }

    // Append the remaining part of the original string.
    decoded.push_str(&str[last_index..]);
    Some(decoded)
}

// Percent-encodes the given string.
fn encode_percent(str: &str) -> String {
    str.chars()
        .map(|c| if util::is_visible_char(c) { c.to_string() } else { format!("%{:02x}", c as u8) })
        .collect::<Vec<_>>()
        .join("")
}
