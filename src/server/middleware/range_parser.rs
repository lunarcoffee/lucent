use crate::http::headers::Headers;
use crate::server::middleware::{MiddlewareResult, MiddlewareOutput};
use crate::consts;
use crate::http::response::Status;
use crate::util;
use crate::util::Range;
use crate::http::message::Body;
use async_std::io::prelude::ReadExt;

pub enum RangeBody {
    Entire,
    Range(Range, String),
    MultipartRange(Vec<u8>, String),
}

pub struct RangeParser<'a, 'b, 'c> {
    headers: &'a Headers,
    body: &'b mut Body,
    body_len: usize,
    media_type: &'c str,
}

impl<'a, 'b, 'c> RangeParser<'a, 'b, 'c> {
    pub async fn new(headers: &'a Headers, body: &'b mut Body, media_type: &'c str) -> RangeParser<'a, 'b, 'c> {
        let body_len = body.len().await;
        RangeParser {
            headers,
            body,
            body_len,
            media_type,
        }
    }

    pub async fn get_body(mut self) -> MiddlewareResult<RangeBody> {
        match self.headers.get(consts::H_RANGE) {
            None => Ok(RangeBody::Entire),
            Some(range) => {
                let range = &range[0];
                if range.len() < 7 || &range[..5] != consts::H_RANGE_UNIT_BYTES {
                    return Ok(RangeBody::Entire);
                }

                let ranges = range[6..].split(',').filter_map(|range| self.parse_range(range)).collect::<Vec<_>>();
                match ranges.len() {
                    0 => Err(MiddlewareOutput::Status(Status::UnsatisfiableRange, false)),
                    1 => Ok(RangeBody::Range(ranges[0], self.get_content_range(&ranges[0]))),
                    _ => {
                        let time = util::get_time_utc();
                        let sep = format!("{:x}", time.timestamp_millis() + time.timestamp_nanos());
                        let content_type = format!("{}; boundary={}", consts::H_MEDIA_MULTIPART_RANGE, &sep);
                        Ok(RangeBody::MultipartRange(self.multipart_range_body(ranges, sep).await, content_type))
                    }
                }
            }
        }
    }

    fn parse_range(&self, range: &str) -> Option<Range> {
        let range = if range.starts_with('-') && range.len() > 1 {
            let high = self.body_len;
            let low = high - range[1..].parse::<usize>().ok()?;
            Range { low, high }
        } else {
            let parts = range.split('-').collect::<Vec<_>>();
            if parts.len() != 2 {
                return None;
            } else {
                let low = parts[0].parse().ok()?;
                let high = if parts[1].is_empty() { self.body_len } else { parts[1].parse::<usize>().ok()? + 1 };
                Range { low, high }
            }
        };

        if range.high <= self.body_len {
            Some(range)
        } else {
            None
        }
    }

    async fn multipart_range_body(&mut self, ranges: Vec<Range>, sep: String) -> Vec<u8> {
        let mut body = vec![];
        match &mut self.body {
            Body::Bytes(bytes) => body = bytes.to_vec(),
            Body::Stream(reader, len) => {
                body.reserve(*len);
                reader.read_exact(&mut body).await.map(|_| ()).unwrap_or(());
            },
        }

        let mut new_body = vec![];
        for range in ranges {
            new_body.extend_from_slice(format!("--{}\r\n", sep).as_bytes());
            new_body.extend_from_slice(format!(
                "{}: {}\r\n{}: {}\r\n\r\n",
                consts::H_CONTENT_TYPE, self.media_type,
                consts::H_CONTENT_RANGE, self.get_content_range(&range)
            ).as_bytes());
            new_body.extend_from_slice(&body[range.low..range.high]);
            new_body.extend_from_slice(b"\r\n");
        }
        new_body.extend_from_slice(format!("--{}--", sep).as_bytes());
        new_body
    }

    fn get_content_range(&self, range: &Range) -> String {
        format!("{} {}-{}/{}", consts::H_RANGE_UNIT_BYTES, range.low, range.high - 1, self.body_len)
    }
}
