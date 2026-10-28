//! Chat frames and commands on the planning WebSocket.

use serde::{Deserialize, Serialize};

use crate::plan_session::attachments::AttachmentInput;
use crate::plan_session::model::{ChatDelta, ChatSnapshot};

/// A chat update or command result sent to a browser.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "frame", rename_all = "snake_case")]
// The snapshot is sent once per connection; boxing it would change the pinned
// `ChatFrame::Snapshot(ChatSnapshot)` constructor the bridge uses.
#[allow(clippy::large_enum_variant)]
pub enum ChatFrame {
    Snapshot(ChatSnapshot),
    Delta(ChatDelta),
    Result(CommandResult),
    QueueReturned { texts: Vec<String> },
}

/// The outcome of a browser command.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CommandResult {
    pub client_id: Option<String>,
    pub ok: bool,
    pub error: Option<String>,
}

/// A chat command sent by a browser.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum ChatCommand {
    Start {
        client_id: Option<String>,
        offer: Option<String>,
        text: Option<String>,
        attachments: Vec<AttachmentInput>,
    },
    Send {
        client_id: Option<String>,
        text: String,
        attachments: Vec<AttachmentInput>,
    },
    Withdraw {
        client_id: Option<String>,
        queue_id: String,
    },
    Permission {
        client_id: Option<String>,
        request_id: String,
        allow: bool,
        note: Option<String>,
    },
    Interrupt {
        client_id: Option<String>,
    },
    End {
        client_id: Option<String>,
    },
}
