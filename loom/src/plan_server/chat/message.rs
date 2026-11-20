//! The checks a chat message passes before a session receives it.

use std::sync::{Arc, PoisonError};

use crate::plan_server::context::PlanContext;
use crate::plan_session::attachments::{
    format_message, resolve_input, without_file_paths, Attachment, AttachmentInput, MAX_ATTACHMENTS,
};
use crate::plan_session::bridge::Bridge;
use crate::plan_session::commands::CommandTable;
use crate::plan_session::input::{
    check_alone, check_delivered, check_send, sanitize, InputRefusal,
};

/// A checked message: what the session receives and its chips.
pub(super) struct Message {
    pub(super) formatted: String,
    pub(super) attachments: Vec<Attachment>,
}

/// The chat session's command table now; a send with no chat session yet starts one, whose
/// list is still loading.
fn chat_table(ctx: &PlanContext) -> Arc<CommandTable> {
    let bridge = ctx.extensions().get::<Bridge>();
    let cell = bridge.and_then(|bridge| bridge.command_cell(&bridge.chat_session_id()?));
    let table = cell.map(|cell| Arc::clone(&cell.read().unwrap_or_else(PoisonError::into_inner)));
    table.unwrap_or_else(|| Arc::new(CommandTable::loading()))
}

/// [`checked_against`] the chat session's command table now.
pub(super) fn checked(
    ctx: &PlanContext,
    text: &str,
    inputs: Vec<AttachmentInput>,
) -> Result<Message, String> {
    checked_against(ctx, text, inputs, &chat_table(ctx))
}

/// Resolve the attachments server-side, then the send checks on what the session receives,
/// a slash message against `table`.
pub(super) fn checked_against(
    ctx: &PlanContext,
    text: &str,
    inputs: Vec<AttachmentInput>,
    table: &CommandTable,
) -> Result<Message, String> {
    if inputs.len() > MAX_ATTACHMENTS {
        return Err(InputRefusal::TooManyAttachments.to_string());
    }
    // Sanitised first: a control character before a `/` would hide the command from the checks.
    let text = sanitize(text);
    check_alone(&text, inputs.len()).map_err(|refusal| refusal.to_string())?;
    let resolved: Result<Vec<Attachment>, _> = {
        let (project, files) = (ctx.project(), &ctx.runtime().attachments);
        inputs
            .into_iter()
            .map(|input| resolve_input(&project, files, input))
            .collect()
    };
    let attachments = resolved.map_err(|error| error.to_string())?;
    check_send(&text, table).map_err(|refusal| refusal.to_string())?;
    let formatted = sanitize(&format_message(&text, &attachments));
    check_delivered(&formatted).map_err(|refusal| refusal.to_string())?;
    // The block names a file's absolute path; the chips browsers get never do.
    let attachments = without_file_paths(attachments);
    Ok(Message {
        formatted,
        attachments,
    })
}
