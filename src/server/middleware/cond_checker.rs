use chrono::{DateTime, Utc};

use crate::consts;
use crate::http::headers::Headers;
use crate::http::response::Status;
use crate::server::middleware::{MiddlewareOutput, MiddlewareResult};
use crate::util;

pub struct CondInfo {
    pub etag: Option<String>,
    pub last_modified: Option<DateTime<Utc>>,
}

impl CondInfo {
    pub fn new(etag: Option<String>, last_modified: Option<DateTime<Utc>>) -> Self {
        CondInfo { etag, last_modified }
    }
}

pub struct ConditionalChecker<'a> {
    info: &'a CondInfo,
    headers: &'a Headers,
}

impl<'a> ConditionalChecker<'a> {
    pub fn new(info: &'a CondInfo, headers: &'a Headers) -> Self {
        ConditionalChecker { info, headers }
    }

    pub fn check(&mut self) -> MiddlewareResult<()> {
        if !self.check_positive_headers() {
            Err(MiddlewareOutput::Status(Status::PreconditionFailed, false))
        } else if !self.check_negative_headers() {
            Err(MiddlewareOutput::Status(Status::NotModified, false))
        } else if !self.check_range_header() {
            Err(MiddlewareOutput::Status(Status::Ok, false))
        } else {
            Ok(())
        }
    }

    fn check_positive_headers(&self) -> bool {
        if let Some(matching) = self.headers.get(consts::H_IF_MATCH) {
            if let Some(etag) = &self.info.etag {
                return matching[0] == "*" || matching.contains(etag);
            }
        } else if let Some(since) = self.headers.get(consts::H_IF_UNMODIFIED_SINCE) {
            if let Some(last_modified) = self.info.last_modified {
                return match util::parse_time_imf(&since[0]) {
                    Some(since) => last_modified <= since,
                    _ => true,
                };
            }
        }
        true
    }

    fn check_negative_headers(&self) -> bool {
        if let Some(not_matching) = self.headers.get(consts::H_IF_NONE_MATCH) {
            if let Some(etag) = &self.info.etag {
                return not_matching[0] != "*" && not_matching.iter().all(|m| m != etag);
            }
        } else if let Some(since) = self.headers.get(consts::H_IF_MODIFIED_SINCE) {
            if let Some(last_modified) = self.info.last_modified {
                return match util::parse_time_imf(&since[0]) {
                    Some(since) => last_modified > since,
                    _ => true,
                };
            }
        }
        true
    }

    fn check_range_header(&self) -> bool {
        if self.headers.contains(consts::H_RANGE) {
            if let Some(etag_or_date) = self.headers.get(consts::H_IF_RANGE) {
                let etag_or_date = &etag_or_date[0];
                if let Some(since) = util::parse_time_imf(etag_or_date) {
                    if let Some(last_modified) = self.info.last_modified {
                        return last_modified <= since;
                    }
                } else if etag_or_date.starts_with("\"") && etag_or_date.ends_with("\"") {
                    if let Some(etag) = &self.info.etag {
                        return etag_or_date == etag;
                    }
                }
            }
        }
        true
    }
}
