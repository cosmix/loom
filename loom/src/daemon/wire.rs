//! Bounded daemon request and response framing.

use super::protocol::{Capability, Request, Response};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::io::{Read, Write};

const REQUEST_PREFACE_MAGIC: [u8; 4] = *b"LOOM";
const REQUEST_PREFACE_VERSION: u8 = 1;
const REQUEST_PREFACE_HEADER_BYTES: usize = 8;

pub const MAX_CREDENTIAL_BYTES: usize = 256;
pub const MAX_REQUEST_BYTES: usize = 64 * 1024;
pub const MAX_RESPONSE_BYTES: usize = 2 * 1024 * 1024;

mod sealed {
    pub trait Sealed {}
}

/// A message supported by Loom's daemon wire protocol.
pub trait WireMessage: sealed::Sealed + Sized {
    fn write_wire<W: Write>(&self, stream: &mut W) -> Result<()>;
    fn read_wire<R: Read>(stream: &mut R) -> Result<Self>;
}

impl sealed::Sealed for Request {}
impl sealed::Sealed for Response {}

impl WireMessage for Request {
    fn write_wire<W: Write>(&self, stream: &mut W) -> Result<()> {
        write_request_preface(stream, self.required_capability(), self.credential())?;
        write_json_frame(stream, self, MAX_REQUEST_BYTES)
    }

    fn read_wire<R: Read>(stream: &mut R) -> Result<Self> {
        let preface = read_request_preface(stream)?;
        let length = read_request_length(stream)?;
        let request = read_request_body(stream, length)?;
        if !preface.matches(&request) {
            bail!("Request preface does not match request body");
        }
        Ok(request)
    }
}

impl WireMessage for Response {
    fn write_wire<W: Write>(&self, stream: &mut W) -> Result<()> {
        write_json_frame(stream, self, MAX_RESPONSE_BYTES)
    }

    fn read_wire<R: Read>(stream: &mut R) -> Result<Self> {
        read_json_frame(stream, MAX_RESPONSE_BYTES)
    }
}

/// Allocation-free authentication metadata read before a request frame.
pub struct RequestPreface {
    capability: Capability,
    credential: [u8; MAX_CREDENTIAL_BYTES],
    credential_len: usize,
}

impl fmt::Debug for RequestPreface {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RequestPreface")
            .field("capability", &self.capability)
            .field("credential", &"[REDACTED]")
            .finish()
    }
}

impl RequestPreface {
    pub fn capability(&self) -> Capability {
        self.capability
    }

    pub fn credential(&self) -> &str {
        std::str::from_utf8(&self.credential[..self.credential_len])
            .expect("validated request credential")
    }

    pub fn matches(&self, request: &Request) -> bool {
        self.capability == request.required_capability()
            && constant_time_eq(self.credential(), request.credential())
    }
}

pub fn write_message<T: WireMessage, W: Write>(stream: &mut W, message: &T) -> Result<()> {
    message.write_wire(stream)?;
    stream.flush().context("Failed to flush stream")
}

pub fn read_message<T: WireMessage, R: Read>(stream: &mut R) -> Result<T> {
    T::read_wire(stream)
}

pub fn read_request_preface<R: Read>(stream: &mut R) -> Result<RequestPreface> {
    let mut header = [0u8; REQUEST_PREFACE_HEADER_BYTES];
    stream
        .read_exact(&mut header)
        .context("Failed to read request authentication preface")?;

    if header[..4] != REQUEST_PREFACE_MAGIC {
        bail!("Invalid request authentication preface");
    }
    if header[4] != REQUEST_PREFACE_VERSION {
        bail!("Unsupported request authentication preface version");
    }
    let capability = decode_capability(header[5])?;
    let credential_len = u16::from_be_bytes([header[6], header[7]]) as usize;
    if credential_len == 0 || credential_len > MAX_CREDENTIAL_BYTES {
        bail!("Invalid request credential length");
    }

    let mut credential = [0u8; MAX_CREDENTIAL_BYTES];
    stream
        .read_exact(&mut credential[..credential_len])
        .context("Failed to read request credential")?;
    std::str::from_utf8(&credential[..credential_len])
        .context("Request credential must be valid UTF-8")?;

    Ok(RequestPreface {
        capability,
        credential,
        credential_len,
    })
}

pub fn read_request_length<R: Read>(stream: &mut R) -> Result<usize> {
    read_frame_length(stream, MAX_REQUEST_BYTES)
}

pub fn read_request_body<R: Read>(stream: &mut R, length: usize) -> Result<Request> {
    if length > MAX_REQUEST_BYTES {
        bail!("Request frame exceeds the configured limit");
    }
    read_json_body(stream, length)
}

fn write_request_preface<W: Write>(
    stream: &mut W,
    capability: Capability,
    credential: &str,
) -> Result<()> {
    let credential_len = credential.len();
    if credential_len == 0 || credential_len > MAX_CREDENTIAL_BYTES {
        bail!("Request credential length is outside the allowed range");
    }
    let mut header = [0u8; REQUEST_PREFACE_HEADER_BYTES];
    header[..4].copy_from_slice(&REQUEST_PREFACE_MAGIC);
    header[4] = REQUEST_PREFACE_VERSION;
    header[5] = encode_capability(capability);
    header[6..].copy_from_slice(&(credential_len as u16).to_be_bytes());
    stream
        .write_all(&header)
        .context("Failed to write request authentication preface")?;
    stream
        .write_all(credential.as_bytes())
        .context("Failed to write request credential")
}

fn write_json_frame<T: Serialize, W: Write>(
    stream: &mut W,
    message: &T,
    max_bytes: usize,
) -> Result<()> {
    let json = serde_json::to_vec(message).context("Failed to serialize message")?;
    if json.len() > max_bytes {
        bail!("Serialized message exceeds the configured frame limit");
    }
    let length = u32::try_from(json.len()).context("Serialized message length overflow")?;
    stream
        .write_all(&length.to_be_bytes())
        .context("Failed to write message length")?;
    stream
        .write_all(&json)
        .context("Failed to write message body")
}

fn read_json_frame<T: for<'de> Deserialize<'de>, R: Read>(
    stream: &mut R,
    max_bytes: usize,
) -> Result<T> {
    let length = read_frame_length(stream, max_bytes)?;
    read_json_body(stream, length)
}

fn read_frame_length<R: Read>(stream: &mut R, max_bytes: usize) -> Result<usize> {
    let mut length = [0u8; 4];
    stream
        .read_exact(&mut length)
        .context("Failed to read message length")?;
    let length = u32::from_be_bytes(length) as usize;
    if length > max_bytes {
        bail!("Message frame exceeds the configured limit");
    }
    Ok(length)
}

fn read_json_body<T: for<'de> Deserialize<'de>, R: Read>(
    stream: &mut R,
    length: usize,
) -> Result<T> {
    let mut json = vec![0u8; length];
    stream
        .read_exact(&mut json)
        .context("Failed to read message body")?;
    serde_json::from_slice(&json).context("Failed to deserialize message")
}

fn encode_capability(capability: Capability) -> u8 {
    match capability {
        Capability::User => 1,
        Capability::Admin => 2,
    }
}

fn decode_capability(encoded: u8) -> Result<Capability> {
    match encoded {
        1 => Ok(Capability::User),
        2 => Ok(Capability::Admin),
        _ => bail!("Invalid request capability"),
    }
}

fn constant_time_eq(left: &str, right: &str) -> bool {
    left.len() == right.len()
        && left
            .as_bytes()
            .iter()
            .zip(right.as_bytes())
            .fold(0u8, |difference, (left, right)| difference | (left ^ right))
            == 0
}

#[cfg(test)]
#[path = "wire_tests.rs"]
mod tests;
