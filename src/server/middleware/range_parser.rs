use crate::http::headers::Headers;
use crate::server::middleware::{MiddlewareResult, MiddlewareOutput};
use crate::consts;
use crate::http::response::Status;
use crate::util;

pub enum RangeBody {
    Entire,
    Range(Vec<u8>, String),
    MultipartRange(Vec<u8>, String),
}

#[derive(Clone, Copy)]
pub struct Range {
    low: usize,
    high: usize,
}

pub struct RangeParser<'a, 'b, 'c> {
    headers: &'a Headers,
    body: &'b Vec<u8>,
    media_type: &'c String,
}

impl<'a, 'b, 'c> RangeParser<'a, 'b, 'c> {
    pub fn new(headers: &'a Headers, body: &'b Vec<u8>, media_type: &'c String) -> Self {
        RangeParser { headers, body, media_type }
    }

    pub fn get_body(self) -> MiddlewareResult<RangeBody> {
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
                    1 => {
                        let body = self.body[ranges[0].low..ranges[0].high].to_vec();
                        Ok(RangeBody::Range(body, self.get_content_range(&ranges[0])))
                    }
                    _ => {
                        let time = util::get_time_utc();
                        let sep = format!("{:x}", time.timestamp_millis() + time.timestamp_nanos());
                        let content_type = format!("{}; boundary={}", consts::H_MEDIA_MULTIPART_RANGE, &sep);
                        Ok(RangeBody::MultipartRange(self.multipart_range_body(ranges, sep), content_type))
                    }
                }
            }
        }
    }

    fn parse_range(&self, range: &str) -> Option<Range> {
        let range = if range.starts_with('-') && range.len() > 1 {
            let high = self.body.len();
            let low = high - range[1..].parse::<usize>().ok()?;
            Range { low, high }
        } else {
            let parts = range.split('-').collect::<Vec<_>>();
            if parts.len() != 2 {
                return None;
            } else {
                let low = parts[0].parse().ok()?;
                let high = if parts[1].is_empty() { self.body.len() } else { parts[1].parse::<usize>().ok()? + 1 };
                Range { low, high }
            }
        };

        if range.high <= self.body.len() {
            Some(range)
        } else {
            None
        }
    }

    fn multipart_range_body(&self, ranges: Vec<Range>, sep: String) -> Vec<u8> {
        let mut body = vec![];
        for range in ranges {
            body.extend_from_slice(format!("--{}\r\n", sep).as_bytes());
            body.extend_from_slice(format!(
                "{}: {}\r\n{}: {}\r\n\r\n",
                consts::H_CONTENT_TYPE, self.media_type,
                consts::H_CONTENT_RANGE, self.get_content_range(&range)
            ).as_bytes());
            body.extend_from_slice(&self.body[range.low..range.high]);
            body.extend_from_slice(b"\r\n");
        }
        body.extend_from_slice(format!("--{}--", sep).as_bytes());
        body
    }

    fn get_content_range(&self, range: &Range) -> String {
        format!("{} {}-{}/{}", consts::H_RANGE_UNIT_BYTES, range.low, range.high - 1, self.body.len())
    }
}
