use pwhash::bcrypt;

use crate::{
    consts,
    http::{message::MessageBuilder, request::Request, response::{Response, Status}},
    log,
    server::{
        config::{Config, realm_info::{Credentials, RealmInfo}},
        middleware::{MiddlewareOutput, MiddlewareResult},
    },
};

// Authenticates the `request` using HTTP basic authentication, checking against credentials in the `config`.
pub struct BasicAuthChecker<'a> {
    request: &'a Request,
    config: &'a Config,
}

impl<'a> BasicAuthChecker<'a> {
    pub fn new(request: &'a Request, config: &'a Config) -> Self {
        BasicAuthChecker { request, config }
    }

    // Checks if authentication is required, sending a 401 with a challenge if necessary.
    pub fn check(&self) -> MiddlewareResult<bool> {
        let target = self.request.uri.to_string();

        // Check if the request's target matches a route in an authentication realm.
        for (realm, RealmInfo { credentials, routes }) in &self.config.basic_auth {
            if routes.iter().any(|r| r.0.captures(&target).is_some()) {
                // If it does, check if information is already provided (in the 'Authorization' header). Use that if
                // available, and send a challenge otherwise.
                return match self.request.headers.get(consts::H_AUTHORIZATION) {
                    Some(auth) => self.check_auth_header(&auth, realm, &credentials),
                    _ => self.www_authenticate_output(realm),
                };
            }
        }

        // The requested resource does not require authentication.
        Ok(false)
    }

    // Checks the request's `Authorization` header.
    fn check_auth_header(
        &self,
        auth: &Vec<String>,
        realm: &str,
        realm_credentials: &Vec<Credentials>,
    ) -> MiddlewareResult<bool> {

        // Attempt to parse the 'Authorization' header, ensuring the client is using basic authentication.
        let auth = auth[0].splitn(2, ' ').collect::<Vec<_>>();
        if auth.len() > 1 && auth[0].eq_ignore_ascii_case(consts::H_AUTH_BASIC) {
            // Try decoding the base64-encoded credentials.
            if let Ok(Ok(user_credentials)) = base64::decode(&auth[1]).map(|c| String::from_utf8(c)) {
                // Try parsing the colon-delimited username and password.
                let credentials = user_credentials.splitn(2, ':').collect::<Vec<_>>();
                if credentials.len() > 1 {
                    let user = credentials[0];
                    let password = credentials[1];

                    // If the request's credentials match a set of valid credentials, authentication is successful.
                    for c in realm_credentials {
                        if c.user == user && bcrypt::verify(password, &c.password_hash) {
                            return Ok(true);
                        }
                    }
                }
            }
        }

        // Authentication failed, send challenge.
        self.www_authenticate_output(realm)
    }

    // Generates an authentication challenge.
    fn www_authenticate_output(&self, realm: &str) -> MiddlewareResult<bool> {
        log::info(format!("({}) {} {}", Status::Unauthorized, self.request.method, self.request.uri));

        let auth = format!("{} {}=\"{}\"", consts::H_AUTH_BASIC, consts::H_AUTH_REALM, realm);
        let response = MessageBuilder::<Response>::new()
            .with_status(Status::Unauthorized)
            .with_header(consts::H_WWW_AUTHENTICATE, &auth)
            .build();
        Err(MiddlewareOutput::Response(response, false))
    }
}
