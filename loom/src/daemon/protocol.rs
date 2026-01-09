use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};

/// Client request to daemon
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Request {
    /// Subscribe to live status updates
    SubscribeStatus,
    /// Subscribe to raw log stream
    SubscribeLogs,
    /// Request daemon shutdown
    Stop,
    /// Disconnect cleanly
    Unsubscribe,
    /// Ping to check if daemon is alive
    Ping,
}

/// Daemon response to client
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Response {
    Ok,
    Error {
        message: String,
    },
    StatusUpdate {
        stages_executing: Vec<StageInfo>,
        stages_pending: Vec<String>,
        stages_completed: Vec<String>,
        stages_blocked: Vec<String>,
    },
    LogLine {
        line: String,
    },
    Pong,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StageInfo {
    pub id: String,
    pub name: String,
    pub session_pid: Option<u32>,
    pub started_at: DateTime<Utc>,
}

/// Write a length-prefixed JSON message to a stream.
///
/// Format: 4-byte big-endian length prefix + JSON data
///
/// # Arguments
/// * `stream` - The stream to write to
/// * `message` - The message to serialize and write
///
/// # Returns
/// `Ok(())` on success, error if serialization or write fails
pub fn write_message<T: Serialize, W: Write>(stream: &mut W, message: &T) -> Result<()> {
    let json = serde_json::to_vec(message).context("Failed to serialize message")?;
    let len = json.len() as u32;
    let len_bytes = len.to_be_bytes();

    stream
        .write_all(&len_bytes)
        .context("Failed to write message length")?;
    stream
        .write_all(&json)
        .context("Failed to write message body")?;
    stream.flush().context("Failed to flush stream")?;

    Ok(())
}

/// Read a length-prefixed JSON message from a stream.
///
/// Format: 4-byte big-endian length prefix + JSON data
///
/// # Arguments
/// * `stream` - The stream to read from
///
/// # Returns
/// `Ok(T)` with the deserialized message on success, error if read or deserialization fails
pub fn read_message<T: for<'de> Deserialize<'de>, R: Read>(stream: &mut R) -> Result<T> {
    let mut len_bytes = [0u8; 4];
    stream
        .read_exact(&mut len_bytes)
        .context("Failed to read message length")?;
    let len = u32::from_be_bytes(len_bytes) as usize;

    // Sanity check: prevent DOS via huge length claim (max 10 MB)
    if len > 10 * 1024 * 1024 {
        anyhow::bail!("Message too large: {len} bytes");
    }

    let mut json_bytes = vec![0u8; len];
    stream
        .read_exact(&mut json_bytes)
        .context("Failed to read message body")?;

    serde_json::from_slice(&json_bytes).context("Failed to deserialize message")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn test_write_and_read_request() {
        let mut buffer = Vec::new();
        let request = Request::Ping;

        write_message(&mut buffer, &request).expect("Failed to write message");

        let mut cursor = Cursor::new(buffer);
        let decoded: Request = read_message(&mut cursor).expect("Failed to read message");

        match decoded {
            Request::Ping => {}
            _ => panic!("Expected Ping request"),
        }
    }

    #[test]
    fn test_write_and_read_response() {
        let mut buffer = Vec::new();
        let response = Response::Pong;

        write_message(&mut buffer, &response).expect("Failed to write message");

        let mut cursor = Cursor::new(buffer);
        let decoded: Response = read_message(&mut cursor).expect("Failed to read message");

        match decoded {
            Response::Pong => {}
            _ => panic!("Expected Pong response"),
        }
    }

    #[test]
    fn test_write_and_read_status_update() {
        let mut buffer = Vec::new();
        let response = Response::StatusUpdate {
            stages_executing: vec![StageInfo {
                id: "stage-1".to_string(),
                name: "Test Stage".to_string(),
                session_pid: Some(12345),
                started_at: Utc::now(),
            }],
            stages_pending: vec!["stage-2".to_string()],
            stages_completed: vec!["stage-0".to_string()],
            stages_blocked: vec![],
        };

        write_message(&mut buffer, &response).expect("Failed to write message");

        let mut cursor = Cursor::new(buffer);
        let decoded: Response = read_message(&mut cursor).expect("Failed to read message");

        match decoded {
            Response::StatusUpdate {
                stages_executing, ..
            } => {
                assert_eq!(stages_executing.len(), 1);
                assert_eq!(stages_executing[0].id, "stage-1");
            }
            _ => panic!("Expected StatusUpdate response"),
        }
    }

    #[test]
    fn test_read_message_too_large() {
        let mut buffer = Vec::new();
        let len: u32 = 20 * 1024 * 1024; // 20 MB (exceeds 10 MB limit)
        buffer.extend_from_slice(&len.to_be_bytes());

        let mut cursor = Cursor::new(buffer);
        let result: Result<Request> = read_message(&mut cursor);

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("too large"));
    }
}
