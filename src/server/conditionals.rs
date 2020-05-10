use chrono::{DateTime, Utc};
use crate::http::headers::Headers;
use crate::http::consts;
use crate::util;

pub struct ConditionalInformation {
    pub etag: Option<String>,
    pub last_modified: Option<DateTime<Utc>>,
}

#[derive(Copy, Clone)]
pub enum ConditionalCheckResult {
    Pass,
    FailPositive,
    FailNegative,
    FailRange,
}

pub struct ConditionalChecker<'a, 'b> {
    info: &'a ConditionalInformation,
    headers: &'b Headers,
}

impl ConditionalChecker<'_, '_> {
    pub fn new<'a, 'b>(info: &'a ConditionalInformation, headers: &'b Headers) -> ConditionalChecker<'a, 'b> {
        ConditionalChecker { info, headers }
    }

    pub fn check(&self) -> ConditionalCheckResult {
        if !self.check_positive_headers() {
            ConditionalCheckResult::FailPositive
        } else if !self.check_negative_headers() {
            ConditionalCheckResult::FailNegative
        } else if !self.check_range_header() {
            ConditionalCheckResult::FailRange
        } else {
            ConditionalCheckResult::Pass
        }
    }

    fn check_positive_headers(&self) -> bool {
        if let Some(matching) = self.headers.get(consts::H_IF_MATCH) {
            if let Some(etag) = &self.info.etag {
                return matching[0] == "*" || matching.contains(etag);
            }
        } else if let Some(since) = self.headers.get(consts::H_IF_UNMODIFIED_SINCE) {
            if let Some(last_modified) = self.info.last_modified {
                let since = match util::parse_time_imf(&since[0]) {
                    Some(since) => since,
                    _ => return true,
                };
                return last_modified <= since;
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
                let since = match util::parse_time_imf(&since[0]) {
                    Some(since) => since,
                    _ => return true,
                };
                return last_modified > since;
            }
        }
        true
    }

    fn check_range_header(&self) -> bool {
        // TODO:
        true
    }
}
