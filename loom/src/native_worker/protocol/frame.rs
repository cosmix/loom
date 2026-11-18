//! Bounded JSON-lines framing, frame classification and the transport trait.

use serde_json::Value;
use std::io::{self, BufRead, Write};
use std::time::Duration;

#[derive(Debug)]
pub enum FrameError {
    Oversized,
    Eof,
    Io(io::Error),
    Malformed(String),
}

impl std::fmt::Display for FrameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Oversized => write!(f, "frame exceeds size limit"),
            Self::Eof => write!(f, "end of frame stream"),
            Self::Io(error) => write!(f, "frame I/O error: {error}"),
            Self::Malformed(message) => write!(f, "malformed frame: {message}"),
        }
    }
}

impl std::error::Error for FrameError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

/// Reads one bounded line, consuming an over-long line through its newline.
pub fn read_frame<R: BufRead>(reader: &mut R, max: usize) -> Result<Value, FrameError> {
    let mut buf = Vec::new();
    if !crate::process::lines::read_bounded_line(reader, &mut buf, max.saturating_add(1))
        .map_err(FrameError::Io)?
    {
        return Err(FrameError::Eof);
    }
    if buf.len() > max {
        return Err(FrameError::Oversized);
    }
    if buf.is_empty() {
        return Err(FrameError::Malformed("empty line".into()));
    }
    serde_json::from_slice(&buf).map_err(|error| FrameError::Malformed(error.to_string()))
}

pub fn write_frame<W: Write>(writer: &mut W, frame: &Value) -> io::Result<()> {
    let mut buf = serde_json::to_vec(frame).map_err(io::Error::other)?;
    buf.push(b'\n');
    writer.write_all(&buf)?;
    writer.flush()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Incoming {
    Response {
        id: u64,
        result: Result<Value, RpcError>,
    },
    Notification {
        method: String,
        params: Value,
    },
    ServerRequest {
        id: Value,
        method: String,
        params: Value,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RpcError {
    pub code: i64,
    pub message: String,
}

pub fn classify(frame: Value) -> Result<Incoming, FrameError> {
    let object = frame
        .as_object()
        .ok_or_else(|| FrameError::Malformed("frame is not an object".into()))?;
    let method = object
        .get("method")
        .map(|value| {
            value
                .as_str()
                .ok_or_else(|| FrameError::Malformed("method is not a string".into()))
        })
        .transpose()?;
    let id = object.get("id").filter(|id| !id.is_null());
    let params = object.get("params").cloned().unwrap_or(Value::Null);
    match (method, id) {
        (Some(method), Some(id)) => Ok(Incoming::ServerRequest {
            id: id.clone(),
            method: method.into(),
            params,
        }),
        (Some(method), None) => Ok(Incoming::Notification {
            method: method.into(),
            params,
        }),
        (None, Some(id)) => classify_response(object, id),
        (None, None) => Err(FrameError::Malformed(
            "frame has neither method nor id".into(),
        )),
    }
}

fn classify_response(
    object: &serde_json::Map<String, Value>,
    id: &Value,
) -> Result<Incoming, FrameError> {
    let id = id
        .as_u64()
        .ok_or_else(|| FrameError::Malformed("response id is not an unsigned integer".into()))?;
    let result = if let Some(result) = object.get("result") {
        Ok(result.clone())
    } else {
        let error = object
            .get("error")
            .and_then(Value::as_object)
            .ok_or_else(|| FrameError::Malformed("response has no valid error or result".into()))?;
        let code = error
            .get("code")
            .and_then(Value::as_i64)
            .ok_or_else(|| FrameError::Malformed("error code is not an integer".into()))?;
        let message = error
            .get("message")
            .and_then(Value::as_str)
            .ok_or_else(|| FrameError::Malformed("error message is not a string".into()))?;
        Err(RpcError {
            code,
            message: message.into(),
        })
    };
    Ok(Incoming::Response { id, result })
}

/// Interface used by the supervisor to exchange frames.
pub trait Transport {
    fn send(&mut self, frame: &Value) -> io::Result<()>;
    /// Returns `Ok(None)` on timeout.
    fn recv(&mut self, timeout: Duration) -> Result<Option<Value>, FrameError>;
}

#[cfg(test)]
#[path = "frame_tests.rs"]
mod tests;
