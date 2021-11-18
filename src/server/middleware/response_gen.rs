use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
};

use async_std::{
    fs::{File, Metadata},
    io::{prelude::SeekExt, SeekFrom},
    path::Path,
};
use chrono::{DateTime, Utc};

use crate::{
    consts,
    http::{
        message::{Body, MessageBuilder},
        request::{Method, Request},
        response::{Response, Status},
        uri::Uri,
    },
    log,
    server::{
        config::{route_replacement::RouteReplacement, route_spec::RouteSpec, Config},
        file_server::ConnInfo,
        middleware::{
            basic_auth::BasicAuthChecker,
            cgi_runner::CgiRunner,
            cond_checker::{CondInfo, ConditionalChecker},
            dir_lister::DirectoryLister,
            range_parser::{RangeBody, RangeParser},
            MiddlewareOutput, MiddlewareResult,
        },
        template::{templates::Templates, SubstitutionMap, TemplateSubstitution},
    },
    util,
};

// This generates an appropriate response for a single request.
pub struct ResponseGenerator<'a> {
    config: &'a Config,
    templates: &'a Templates,

    request: &'a mut Request,
    conn_info: &'a ConnInfo,

    // The request's target, as originally specified in the request.
    raw_target: String,

    // The target after any potential URL rewriting.
    routed_target: String,

    // The path of the resource `routed_target` refers to.
    target_file: String,

    response: MessageBuilder<Response>,
    body: Body,
    media_type: String,
}

impl<'a> ResponseGenerator<'a> {
    pub fn new(config: &'a Config, templates: &'a Templates, request: &'a mut Request, conn: &'a ConnInfo) -> Self {
        // This also does URL rewriting.
        let (raw_target, routed_target, target_file) = Self::get_req_targets(request, config);

        ResponseGenerator {
            config,
            templates,

            request,
            conn_info: conn,

            raw_target,
            routed_target,
            target_file,

            response: MessageBuilder::<Response>::new(),

            // These are just defaults.
            body: Body::Bytes(vec![]),
            media_type: consts::H_MEDIA_BINARY.to_string(),
        }
    }

    pub async fn get_response(mut self) -> MiddlewareResult<()> {
        // Check authentication; any authentication challenges will be propagated upwards.
        let required_auth = BasicAuthChecker::new(self.request, self.config).check()?;

        let file = match File::open(&self.target_file).await {
            Ok(file) => file,
            _ => return Err(MiddlewareOutput::Error(Status::NotFound, false)),
        };

        // Get the information used to check conditional headers and generate the response body.
        let metadata = file.metadata().await?;
        let last_modified = Some(metadata.modified()?.into());
        let etag = Some(Self::generate_etag(&last_modified.unwrap()));
        let info = CondInfo::new(etag, last_modified);
        self.set_body(&info, &metadata).await?;

        let response = self
            .response
            // Allow the client to make conditional requests.
            .with_header(consts::H_ETAG, &info.etag.unwrap())
            .with_header(consts::H_LAST_MODIFIED, &util::format_time_rfc2616(&info.last_modified.unwrap().into()))
            .with_body(self.body, &self.media_type)
            .build();

        // Log the request. Show the original and routed targets if URL rewriting occurred, and also show whether basic
        // authentication was used.
        let host = self.request.headers.get_host().unwrap();
        let reroute =
            if self.raw_target != self.routed_target { format!(" -> {}", self.routed_target) } else { String::new() };
        let auth = if required_auth { " (basic auth)" } else { "" };
        log::req(response.status, self.request.method, &self.raw_target, &(reroute + auth), host);

        // Return the response in a `MiddlewareOutput`; this will be sent by an `OutputProcessor`.
        Err(MiddlewareOutput::Response(response, false))
    }

    // Set the body based on the type of resource requested (file/directory).
    async fn set_body(&mut self, info: &CondInfo, metadata: &Metadata) -> MiddlewareResult<()> {
        // Only support GET and HEAD requests to static resources.
        if self.request.method != Method::Get && self.request.method != Method::Head {
            // Instead of immediately sending a 405, try allowing a CGI script to respond to this request.
            return self
                .set_file_body(true, info)
                .await
                .and(Err(MiddlewareOutput::Status(Status::MethodNotAllowed, false)));
        }

        // Send a directory listing if it is enabled and the targeted resource is a directory.
        if metadata.is_dir() {
            if self.config.dir_listing.enabled {
                self.media_type = consts::H_MEDIA_HTML.to_string();
                let listing = DirectoryLister::new(&self.routed_target, &self.target_file, self.templates, self.config)
                    .get_listing_body()
                    .await?
                    .into_bytes();
                self.body = Body::Bytes(listing);
            } else {
                // If directory listing is disabled, act as if no such resource exists.
                return Err(MiddlewareOutput::Error(Status::NotFound, false));
            }
        } else {
            // Otherwise, the resource is a file (or symlink).
            self.set_file_body(false, info).await?;
        }
        Ok(())
    }

    // The targeted resource is a file or symlink. This sets the body to the contents of the file, and in the case of a
    // CGI script, the result of its execution.
    async fn set_file_body(&mut self, cgi: bool, info: &CondInfo) -> MiddlewareResult<()> {
        let target = &self.target_file;
        let path = Path::new(target);

        let file_ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
        let target_no_ext = &target[..target.len() - file_ext.len() - 1];

        // If the file's name (w/o an extension) ends with '_cgi', it is a CGI script.
        if target_no_ext.ends_with("_cgi") {
            // If it ends with '_nph_cgi', it is an NPH script.
            let is_nph = target_no_ext.ends_with("_nph_cgi");

            // Execute the script. If it exits successfully, the `MiddlewareOutput` with the result will propagate
            // upwards and be sent.
            CgiRunner::new(&target, &mut self.request, &self.conn_info, &self.config, is_nph)
                .script_response()
                .await?;
        }

        // Check conditional headers and set the body for non-script files.
        if !cgi {
            ConditionalChecker::new(info, &mut self.request.headers).check()?;
            self.media_type = util::media_type_by_ext(file_ext).to_string();

            // Don't add a body to HEAD requests.
            if self.request.method != Method::Head {
                let file = File::open(&target).await?;
                let len = file.metadata().await?.len();
                self.body = Body::Stream(file, len as usize);

                // Set the correct body in the case that this is a range request.
                self.set_range_body().await?;
            }
        }
        Ok(())
    }

    // Parse the ranges requested (if present) and modify the body accordingly.
    async fn set_range_body(&mut self) -> MiddlewareResult<()> {
        match RangeParser::new(&self.request.headers, &mut self.body, &self.media_type)
            .await
            .get_body()
            .await
        {
            Err(output) => return Err(output),
            // A single byte range.
            Ok(RangeBody::Range(range, content_range)) => {
                match &mut self.body {
                    // Just slice the bytes if we have them.
                    Body::Bytes(bytes) => self.body = Body::Bytes(bytes[range.low..range.high].to_vec()),
                    // If we have a file, seek it to the start of the range and set the length to that of the range.
                    Body::Stream(file, len) => {
                        file.seek(SeekFrom::Start(range.low as u64)).await?;
                        *len = range.high - range.low;
                    }
                };
                self.response.set_header(consts::H_CONTENT_RANGE, &content_range);
                self.response.set_status(Status::PartialContent);
            }
            // Multiple ranges, using multipart MIME type. The entire body is generated ahead of time, just set it.
            Ok(RangeBody::MultipartRange(body, media_type)) => {
                self.body = Body::Bytes(body);
                self.media_type = media_type;
                self.response.set_status(Status::PartialContent);
            }
            _ => {}
        }
        Ok(())
    }

    // Gets the request's original target, the target after URL rewriting, and the path for the resource the rewritten
    // target points to.
    fn get_req_targets(request: &mut Request, config: &Config) -> (String, String, String) {
        let raw_target = request.uri.to_string();
        let routed_target = Self::rewrite_url(config, &raw_target).unwrap_or(raw_target.to_string());

        let target_file = match Uri::from(&request.method, &routed_target) {
            Ok(uri) => {
                request.uri = uri;
                format!("{}/{}", &config.file_root, request.uri.to_string_no_query())
            }
            _ => format!("{}{}", &config.file_root, &routed_target),
        };
        (raw_target, routed_target, target_file)
    }

    // Rewrite the given URL (`raw_target`) using the configured routing table. If no rule in the table matches the
    // URL, `None` is returned.
    fn rewrite_url(config: &Config, raw_target: &str) -> Option<String> {
        for (RouteSpec(rule_regex), RouteReplacement(replacement)) in &config.routing_table {
            // Rewrite with the first matching `RouteSpec`; regex captures correspond to the path variables.
            if let Some(capture) = rule_regex.captures(raw_target) {
                // Create the `SubstitutionMap` for rewriting this URL. Start by going over the regex's captures and
                // their corresponding placeholder names.
                let sub = capture
                    .iter()
                    .zip(rule_regex.capture_names())
                    // Skip the first one; that capture has the entire match.
                    .skip(1)
                    // For every capture, turn the corresponding placeholder name and value into an entry; i.e., use
                    // that captured value when substituting that placeholder.
                    .flat_map(|(captures, name)| {
                        captures.into_iter().map(move |c| {
                            (name.unwrap().to_string(), TemplateSubstitution::Single(c.as_str().to_string()))
                        })
                    })
                    .collect::<SubstitutionMap>();

                // Find the end of the match; if this `RouteSpec` only matches a prefix, the remaining text should be
                // retained after rewriting (i.e. if '/hello/world' matches a rule for {'/hello' -> '/bye'}, the result
                // should be '/bye/world' and not '/bye', even though only the '/hello' prefix matched the regex).
                let end_match = rule_regex.find(raw_target).unwrap().end();

                // Rewrite the URL and add any remaining unmatched part.
                return Some(replacement.substitute(&sub)? + &raw_target[end_match..]);
            }
        }

        // This URL is not rewritten.
        None
    }

    // Generate an entity-tag for a resource given its last modified time. This is a weak ETag... but we treat it like
    // a strong one anyway.
    fn generate_etag(modified: &DateTime<Utc>) -> String {
        let mut hasher = DefaultHasher::new();

        // Start with the hash of the time as a string.
        let time = util::format_time_rfc2616(modified);
        time.hash(&mut hasher);
        let etag = format!("\"{:x}", hasher.finish());

        // Add on the hash of the reversed time string.
        time.chars().into_iter().rev().collect::<String>().hash(&mut hasher);
        etag + &format!("{:x}\"", hasher.finish())
    }
}
