use std::fmt::{self, Formatter};

use serde::{
    de::{Error, MapAccess, Visitor},
    Deserialize, Deserializer,
};

use crate::server::config::route_spec::RouteSpec;

// A username and password hash (using bcrypt).
#[derive(Clone)]
pub struct Credentials {
    pub user: String,
    pub password_hash: String,
}

// The credentials allowed for authentication in a realm, along with the routes which are in the realm.
#[derive(Clone)]
pub struct RealmInfo {
    pub credentials: Vec<Credentials>,

    // If the target route for any request matches one of these, the client will be required to authenticate. If they
    // provide information that matches an entry in `credentials`, the challenge is successful.
    pub routes: Vec<RouteSpec>,
}

impl<'a> Deserialize<'a> for RealmInfo {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'a>,
    {
        deserializer.deserialize_map(RealmInfoStringVisitor)
    }
}

pub struct RealmInfoStringVisitor;

impl<'a> Visitor<'a> for RealmInfoStringVisitor {
    type Value = RealmInfo;

    fn expecting(&self, formatter: &mut Formatter) -> fmt::Result {
        formatter.write_str("a map containing credentials and routes for a given realm")
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'a>,
    {
        // Parse the list of credentials (see '/resources/config.yaml'), ensuring that the key in the map entry is
        // correctly named.
        let credentials = map
            .next_entry::<String, Vec<String>>()?
            .filter(|(key, _)| key == "credentials")
            .and_then(|(_, raw)| raw.into_iter().map(parse_credentials).collect::<Option<Vec<_>>>())
            .ok_or(A::Error::custom("expected credentials"))?;

        // Parse the list of routes, ensuring that the key in the map entry is correctly named.
        let routes = map
            .next_entry::<String, Vec<RouteSpec>>()?
            .filter(|(key, _)| key == "routes")
            .ok_or(A::Error::custom("expected routes"))?
            .1;

        Ok(RealmInfo { credentials, routes })
    }
}

// `credentials_str` contains the username and password hash, delimited by a colon (':').
fn parse_credentials(credentials_str: String) -> Option<Credentials> {
    let split = credentials_str.trim().splitn(2, ':').collect::<Vec<_>>();
    (split.len() == 2).then(|| Credentials { user: split[0].to_string(), password_hash: split[1].to_string() })
}
