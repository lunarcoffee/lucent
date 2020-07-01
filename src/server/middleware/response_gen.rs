use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use async_std::fs::{File, Metadata};
use async_std::io::{prelude::SeekExt, SeekFrom};
use async_std::path::Path;
use chrono::{DateTime, Utc};

use crate::{log, util};
use crate::consts;
use crate::http::message::{Body, MessageBuilder};
use crate::http::request::{Method, Request};
use crate::http::response::{Response, Status};
use crate::http::uri::Uri;
use crate::server::config::Config;
use crate::server::config::route_replacement::RouteReplacement;
use crate::server::config::route_spec::RouteSpec;
use crate::server::file_server::ConnInfo;
use crate::server::middleware::{MiddlewareOutput, MiddlewareResult};
use crate::server::middleware::basic_auth::BasicAuthChecker;
use crate::server::middleware::cgi_runner::CgiRunner;
use crate::server::middleware::cond_checker::{CondInfo, ConditionalChecker};
use crate::server::middleware::dir_lister::DirectoryLister;
use crate::server::middleware::range_parser::{RangeBody, RangeParser};
use crate::server::template::{SubstitutionMap, TemplateSubstitution};
use crate::server::template::templates::Templates;

pub struct ResponseGenerator<'a> {
    config: &'a Config,
    templates: &'a Templates,

    request: &'a mut Request,
    conn_info: &'a ConnInfo,
    raw_target: String,
    routed_target: String,
    target: String,

    response: MessageBuilder<Response>,
    body: Body,
    media_type: String,
}

impl<'a> ResponseGenerator<'a> {
    pub fn new(config: &'a Config, templates: &'a Templates, request: &'a mut Request, conn: &'a ConnInfo) -> Self {
        let (raw_target, routed_target, target) = rewrite_url(request, config);

        ResponseGenerator {
            config,
            templates,

            request,
            conn_info: conn,
            raw_target,
            routed_target,
            target,

            response: MessageBuilder::<Response>::new(),
            body: Body::Bytes(vec![]),
            media_type: consts::H_MEDIA_BINARY.to_string(),
        }
    }

    pub async fn get_response(mut self) -> MiddlewareResult<()> {
        let required_auth = BasicAuthChecker::new(self.request, self.config).check()?;

        let file = match File::open(&self.target).await {
            Ok(file) => file,
            _ => return Err(MiddlewareOutput::Error(Status::NotFound, false)),
        };

        let metadata = file.metadata().await?;
        let last_modified = Some(metadata.modified()?.into());
        let etag = Some(Self::generate_etag(&last_modified.unwrap()));
        let info = CondInfo::new(etag, last_modified);
        self.set_body(&info, &metadata).await?;

        let response = self
            .response
            .with_header(consts::H_ETAG, &info.etag.unwrap())
            .with_header(consts::H_LAST_MODIFIED, &util::format_time_imf(&info.last_modified.unwrap().into()))
            .with_body(self.body, &self.media_type)
            .build();

        let routed = self.routed_target;
        let reroute = if self.raw_target != routed { format!(" -> {}", routed) } else { String::new() };
        let auth = if required_auth { " (basic auth)" } else { "" };
        log::info(format!("({}) {} {}{}{}", response.status, &self.request.method, &self.raw_target, reroute, auth));

        Err(MiddlewareOutput::Response(response, false))
    }

    async fn set_body(&mut self, info: &CondInfo, metadata: &Metadata) -> MiddlewareResult<()> {
        if self.request.method != Method::Get && self.request.method != Method::Head {
            return self
                .set_file_body(true, info, metadata)
                .await
                .and(Err(MiddlewareOutput::Status(Status::MethodNotAllowed, false)));
        }

        if metadata.is_dir() {
            self.media_type = consts::H_MEDIA_HTML.to_string();
            self.body = Body::Bytes(DirectoryLister::new(&self.routed_target, &self.target, self.templates)
                .get_listing_body()
                .await?
                .into_bytes());
        } else {
            self.set_file_body(false, info, metadata).await?;
        }
        Ok(())
    }

    async fn set_file_body(&mut self, cgi: bool, info: &CondInfo, metadata: &Metadata) -> MiddlewareResult<()> {
        let target = &self.target;
        let path = Path::new(target);
        let file_ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
        let target_no_ext = &target[..target.len() - file_ext.len() - 1];

        if target_no_ext.ends_with("_cgi") {
            let is_nph = target_no_ext.ends_with("_nph_cgi");
            CgiRunner::new(&self.target, &mut self.request, &self.conn_info, &self.config, is_nph)
                .get_response()
                .await?;
        }

        if !cgi {
            let can_send_range = match ConditionalChecker::new(info, &self.request.headers).check() {
                Err(MiddlewareOutput::Status(Status::Ok, ..)) => false,
                Err(output) if !metadata.is_dir() => return Err(output),
                _ => true,
            };

            self.media_type = util::media_type_by_ext(file_ext).to_string();
            if self.request.method != Method::Head {
                let file = File::open(&self.target).await?;
                let len = file.metadata().await?.len();
                self.body = Body::Stream(file, len as usize);
                if can_send_range {
                    self.set_range_body().await?;
                }
            }
        }
        Ok(())
    }

    async fn set_range_body(&mut self) -> MiddlewareResult<()> {
        match RangeParser::new(&self.request.headers, &mut self.body, &self.media_type).await.get_body().await {
            Err(output) => return Err(output),
            Ok(RangeBody::Range(range, content_range)) => {
                match &mut self.body {
                    Body::Bytes(bytes) => self.body = Body::Bytes(bytes[range.low..range.high].to_vec()),
                    Body::Stream(file, len) => {
                        file.seek(SeekFrom::Start(range.low as u64)).await?;
                        *len = range.high - range.low;
                    }
                };
                self.response.set_header(consts::H_CONTENT_RANGE, &content_range);
                self.response.set_status(Status::PartialContent);
            }
            Ok(RangeBody::MultipartRange(body, media_type)) => {
                self.body = Body::Bytes(body);
                self.media_type = media_type;
                self.response.set_status(Status::PartialContent);
            }
            _ => {}
        }
        Ok(())
    }

    fn generate_etag(modified: &DateTime<Utc>) -> String {
        let mut hasher = DefaultHasher::new();
        let time = util::format_time_imf(modified);
        time.hash(&mut hasher);

        let etag = format!("\"{:x}", hasher.finish());
        time.chars().into_iter().rev().collect::<String>().hash(&mut hasher);

        etag + &format!("{:x}\"", hasher.finish())
    }
}

fn rewrite_url(request: &mut Request, config: &Config) -> (String, String, String) {
    let raw_target = request.uri.to_string();
    let routed_target = route_raw_target(config, &raw_target).unwrap_or(raw_target.to_string());
    let target = format!("{}{}", &config.file_root, &routed_target);
    if let Ok(uri) = Uri::from(&request.method, &routed_target) {
        request.uri = uri;
    }
    (raw_target, routed_target, target)
}

fn route_raw_target(config: &Config, raw_target: &str) -> Option<String> {
    for (RouteSpec(rule_regex), RouteReplacement(replacement)) in &config.routing_table {
        if let Some(capture) = rule_regex.captures(raw_target) {
            let sub = capture.iter().zip(rule_regex.capture_names()).skip(1)
                .map(|(matches, name)| (matches.into_iter(), name.unwrap().to_string()))
                .flat_map(|(captures, name)| captures.map(move |c| (name.to_string(), c.as_str().to_string())))
                .map(|(name, var)| (name, TemplateSubstitution::Single(var)))
                .collect::<SubstitutionMap>();

            let end_match = rule_regex.find(raw_target).unwrap().end();
            return Some(replacement.substitute(&sub)? + &raw_target[end_match..]);
        }
    }
    None
}
