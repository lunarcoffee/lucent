use chrono::{DateTime, Utc};

use crate::consts;
use crate::http::headers::Headers;
use crate::http::response::Status;
use crate::server::middleware::{MiddlewareOutput, MiddlewareResult};
use crate::util;

// Info for a resource (i.e. file) used to determine if the client's cached copy is still up to date.
pub struct CondInfo {
    // This is the entity-tag for the given file; see section 2.3 of RFC 7232.
    pub etag: Option<String>,
    pub last_modified: Option<DateTime<Utc>>,
}

impl CondInfo {
    pub fn new(etag: Option<String>, last_modified: Option<DateTime<Utc>>) -> Self {
        CondInfo { etag, last_modified }
    }
}

// Processes a request's conditional headers, if present.
pub struct ConditionalChecker<'a> {
    info: &'a CondInfo,
    headers: &'a mut Headers,
}

impl<'a> ConditionalChecker<'a> {
    pub fn new(info: &'a CondInfo, headers: &'a mut Headers) -> Self {
        ConditionalChecker { info, headers }
    }

    pub fn check(&mut self) -> MiddlewareResult<()> {
        if !self.check_unchanged_headers() {
            return Err(MiddlewareOutput::Status(Status::PreconditionFailed, false));
        }
        if !self.check_changed_headers() {
            return Err(MiddlewareOutput::Status(Status::NotModified, false));
        }

        if !self.check_range_header() {
            // If the client has an outdated resource, ignore the requested range and send the entire resource.
            self.headers.remove(consts::H_RANGE);
        }
        Ok(())
    }

    // Check headers which check that the resource has not changed. These are typically used in requests that modify a
    // resource, in order to prevent issues related to overwriting other clients' changes (the 'lost update' problem).
    fn check_unchanged_headers(&self) -> bool {
        if let Some(matching) = self.headers.get(consts::H_IF_MATCH) {
            if let Some(etag) = &self.info.etag {
                // If the ETag of the current version of the resource matches one of those provided by the client, the
                // client has the same version of the resource we do, so an update should be fine.
                return matching[0] == "*" || matching.contains(etag);
            }
        } else if let Some(since) = self.headers.get(consts::H_IF_UNMODIFIED_SINCE) {
            if let Some(last_modified) = self.info.last_modified {
                // If the document has not been modified since the client's provided time, they have the latest version
                // of the resource, so an update should be fine. Ignore invalid values.
                return match util::parse_time_imf(&since[0]) {
                    Some(since) => last_modified <= since,
                    _ => true,
                };
            }
        }

        // If there were no conditional headers found, or some were invalid or missing information, just proceed with
        // handling the request.
        true
    }

    // Check headers that check that the resource has changed. If this returns false, the client has an up-to-date copy
    // of the requested resource (we can respond with a 304).
    fn check_changed_headers(&self) -> bool {
        if let Some(not_matching) = self.headers.get(consts::H_IF_NONE_MATCH) {
            if let Some(etag) = &self.info.etag {
                // Only send the resource if none of the client's specified ETags match the current version (i.e. it
                // does not have the current version).
                return not_matching[0] != "*" && not_matching.iter().all(|m| m != etag);
            }
        } else if let Some(since) = self.headers.get(consts::H_IF_MODIFIED_SINCE) {
            if let Some(last_modified) = self.info.last_modified {
                // If the resource has been modified after the client's specified time, their resource is outdated.
                return match util::parse_time_imf(&since[0]) {
                    Some(since) => last_modified > since,
                    _ => true,
                };
            }
        }
        true
    }

    // Checks the 'If-Range' header (see section 3.2 of RFC 7233). In short, the client may send this when they have
    // part of a resource and want the rest, but are unsure if it has been changed. If it is unchanged, just send the
    // parts specified in the 'Range' header; otherwise, send the entire updated resource.
    fn check_range_header(&self) -> bool {
        // Make sure they specify a range as well; it would be pointless to do anything further otherwise.
        if self.headers.contains(consts::H_RANGE) {
            if let Some(etag_or_date) = self.headers.get(consts::H_IF_RANGE) {
                let etag_or_date = &etag_or_date[0];

                // If the client's partial resource is up to date, continue handling the request (return true; this
                // will handle the 'Range' header down the line); otherwise, send the new version (return false).
                if let Some(since) = util::parse_time_imf(etag_or_date) {
                    if let Some(last_modified) = self.info.last_modified {
                        return last_modified == since;
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
