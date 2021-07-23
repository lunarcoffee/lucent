use async_std::io::ErrorKind;
use async_std::io::prelude::ReadExt;

use crate::consts;
use crate::http::headers::Headers;
use crate::http::message::Body;
use crate::http::response::Status;
use crate::server::middleware::{MiddlewareOutput, MiddlewareResult};
use crate::util;
use crate::util::Range;

// The kind of range a request specifies.
pub enum RangeBody {
    // Send the full content of the resource. This is used when there is no 'Range' header in the request.
    Entire,

    // Send the specified range. The string is the value of the `Content-Range` header.
    Range(Range, String),

    // Send the specified range with multipart. The string is the content type.
    MultipartRange(Vec<u8>, String),
}

// Parser for the 'Range' request header.
pub struct RangeParser<'a> {
    headers: &'a Headers,
    body: &'a mut Body,
    body_len: usize,
    media_type: &'a str,
}

impl<'a> RangeParser<'a> {
    pub async fn new(headers: &'a Headers, body: &'a mut Body, media_type: &'a str) -> RangeParser<'a> {
        let body_len = body.len().await;
        RangeParser {
            headers,
            body,
            body_len,
            media_type,
        }
    }

    // Attempts to parse the 'Range' header and return the corresponding `RangeBody`.
    pub async fn get_body(mut self) -> MiddlewareResult<RangeBody> {
        match self.headers.get(consts::H_RANGE) {
            // No header; this is not a range request, send the entire body.
            None => Ok(RangeBody::Entire),
            Some(range) => {
                let range = &range[0];

                // We only support ranges specified in 'bytes', so ignore any other unit.
                if range.len() < 7 || &range[..5] != consts::H_RANGE_UNIT_BYTES {
                    return Ok(RangeBody::Entire);
                }

                // Attempt to parse the specified ranges.
                let ranges = range[6..].split(',').filter_map(|range| self.parse_range(range)).collect::<Vec<_>>();
                match ranges.len() {
                    // The ranges are invalid.
                    0 => Err(MiddlewareOutput::Status(Status::UnsatisfiableRange, false)),
                    1 => Ok(RangeBody::Range(ranges[0], self.get_content_range(&ranges[0]))),
                    _ => {
                        // Generate the multipart boundary (`sep`) and the content type.
                        let time = util::get_time_utc();
                        let sep = format!("{:x}", time.timestamp_millis() + time.timestamp_nanos());
                        let content_type = format!("{}; boundary={}", consts::H_MEDIA_MULTIPART_RANGE, &sep);

                        // Generate the new body to be sent.
                        Ok(RangeBody::MultipartRange(self.multipart_range_body(ranges, sep).await?, content_type))
                    }
                }
            }
        }
    }

    // Attempts to parse a byte range (see section 2.1 of RFC 7233).
    fn parse_range(&self, range: &str) -> Option<Range> {
        let range = if range.starts_with('-') && range.len() > 1 {
            // Suffix byte range; take the last 'n' bytes from the range string '-n'.
            let high = self.body_len;
            let low = high - range[1..].parse::<usize>().ok()?;
            Range { low, high }
        } else {
            // Normal or prefix byte range.
            let parts = range.split('-').collect::<Vec<_>>();
            if parts.len() != 2 {
                return None;
            } else {
                // If `parts[1]` is empty, there was nothing after the '-'; it is a prefix byte range (i.e. 'n-'), so
                // the range takes all bytes after and including byte 'n'.
                let low = parts[0].parse().ok()?;
                let high = if parts[1].is_empty() { self.body_len } else { parts[1].parse::<usize>().ok()? + 1 };
                Range { low, high }
            }
        };

        // Make sure the range is valid.
        if range.high <= self.body_len { Some(range) } else { None }
    }

    // Generate a multipart body for the specified ranges. This is fairly inefficient, as it stores the entire body in
    // memory, which may be fairly large.
    async fn multipart_range_body(&mut self, ranges: Vec<Range>, sep: String) -> MiddlewareResult<Vec<u8>> {
        // Read the entire content of the resource in question into memory.
        let mut body = vec![];
        match &mut self.body {
            Body::Bytes(bytes) => body = bytes.to_vec(),
            Body::Stream(reader, len) => {
                body.reserve(*len);
                if let Err(e) = reader.read_exact(&mut body).await {
                    if e.kind() == ErrorKind::UnexpectedEof {
                        return Err(MiddlewareOutput::Error(Status::InternalServerError, false));
                    }
                }
            }
        }

        // Build up the new body with multipart parts for each range.
        let mut new_body = vec![];
        for range in ranges {
            // Add the part's boundary and some headers.
            new_body.extend_from_slice(format!("--{}\r\n", sep).as_bytes());
            new_body.extend_from_slice(format!(
                "{}: {}\r\n{}: {}\r\n\r\n",
                consts::H_CONTENT_TYPE, self.media_type,
                consts::H_CONTENT_RANGE, self.get_content_range(&range)
            ).as_bytes());

            // Add the actual content of the range.
            new_body.extend_from_slice(&body[range.low..range.high]);
            new_body.extend_from_slice(b"\r\n");
        }

        // Add the terminating boundary.
        new_body.extend_from_slice(format!("--{}--", sep).as_bytes());
        Ok(new_body)
    }

    // Formats the 'Content-Range' header with the given `range`.
    fn get_content_range(&self, range: &Range) -> String {
        format!("{} {}-{}/{}", consts::H_RANGE_UNIT_BYTES, range.low, range.high - 1, self.body_len)
    }
}
