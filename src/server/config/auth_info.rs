use serde::{Deserialize, Deserializer};
use serde::de::{Visitor, SeqAccess, Error};
use serde::export::{Formatter, fmt};

#[derive(Clone)]
pub struct Credentials {
    pub user: String,
    pub password_hash: String,
}

#[derive(Clone)]
pub struct AuthInfo {
    pub realm: String,
    pub credentials: Vec<Credentials>,
}

impl<'a> Deserialize<'a> for AuthInfo {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where D: Deserializer<'a>
    {
        deserializer.deserialize_seq(AuthInfoStringVisitor)
    }
}

pub struct AuthInfoStringVisitor;

impl<'a> Visitor<'a> for AuthInfoStringVisitor {
    type Value = AuthInfo;

    fn expecting(&self, formatter: &mut Formatter) -> fmt::Result {
        formatter.write_str("Sequence of two strings, a realm and a semicolon (`;`) separated list of credentials.")
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, <A as SeqAccess<'a>>::Error>
        where A: SeqAccess<'a>
    {
        let realm = seq.next_element::<String>()?.ok_or(get_error::<A>())?;
        let credentials_str = seq.next_element::<String>()?.ok_or(get_error::<A>())?;
        let credentials = parse_credentials(&credentials_str).ok_or(get_error::<A>())?;
        Ok(AuthInfo { realm, credentials })
    }
}

fn parse_credentials(credentials_str: &str) -> Option<Vec<Credentials>> {
    let credentials = credentials_str
        .split(';')
        .map(|credentials| credentials.trim().splitn(2, ':').collect::<Vec<_>>())
        .try_fold(vec![], |mut acc, c| if c.len() < 2 {
            None
        } else {
            acc.push(Credentials { user: c[0].to_string(), password_hash: c[1].to_string() });
            Some(acc)
        })?;
    Some(credentials)
}

fn get_error<'a, A: SeqAccess<'a>>() -> A::Error {
    A::Error::custom(format!("Authentication information invalid!"))
}
