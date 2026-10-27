//! Browser, public and connection identifiers.

use std::fmt;
use std::fs::File;
use std::io::{self, Read};

use serde::{Deserialize, Serialize};

/// The secret identifier stored in a browser cookie.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct BrowserId(String);

impl BrowserId {
    pub fn mint() -> io::Result<Self> {
        let mut bytes = [0_u8; 16];
        File::open("/dev/urandom")?.read_exact(&mut bytes)?;
        Ok(Self(hex::encode(bytes)))
    }

    pub fn parse(value: &str) -> Option<Self> {
        if value.len() == 32
            && value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            Some(Self(value.to_owned()))
        } else {
            None
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for BrowserId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("BrowserId(..)")
    }
}

/// An independently generated identifier safe to expose to other browsers.
#[derive(Clone, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PublicId(String);

impl PublicId {
    pub fn mint() -> Self {
        let mut bytes = [0_u8; 8];
        File::open("/dev/urandom")
            .and_then(|mut file| file.read_exact(&mut bytes))
            .expect("failed to generate public identifier");
        Self(hex::encode(bytes))
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct ConnId(pub u64);

pub struct ConnCtx {
    pub id: ConnId,
    pub browser: BrowserId,
    pub public_id: PublicId,
}
