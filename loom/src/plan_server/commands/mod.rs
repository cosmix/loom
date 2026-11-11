//! The chat session's command catalogue on the wire: the `GET /api/session/commands` route,
//! the `commands` frame and the owner of the side process that reads the session's list.

mod owner;

use std::sync::Arc;

use crate::context::untrusted::inline_safe;
use crate::plan_server::context::PlanContext;
use crate::plan_server::request::{ApiRequest, ApiResponse};
use crate::plan_server::routes::{BodyKind, Method, Role, RouteSpec};
use crate::plan_server::wire::commands::{CatalogueWireState, CommandsView};
use crate::plan_server::wire::message::PlanMessage;
use crate::plan_session::bridge::Bridge;
use crate::plan_session::commands::{CatalogueState, CommandEntry, CommandTable};

#[cfg(test)]
pub(crate) use self::owner::Builder;
pub(crate) use self::owner::Catalogue;

#[cfg(test)]
mod contract_tests;
#[cfg(test)]
pub(crate) mod tests;

pub fn routes() -> Vec<RouteSpec> {
    vec![RouteSpec {
        method: Method::Get,
        path: "/api/session/commands",
        body: BodyKind::None,
        role: Role::Viewer,
        handler: commands,
    }]
}

/// The session's list: its latest table, else the native rows alone as `loading` (no session
/// parameter, an unknown session, a session whose list has not arrived). Never a 404: the menu
/// always has the native rows to show.
fn commands(req: &ApiRequest<'_>) -> ApiResponse {
    let catalogue = req.ctx.extensions().get::<Catalogue>();
    let session = req.query.get("session");
    let table = catalogue
        .zip(session)
        .and_then(|(catalogue, id)| catalogue.table(id));
    let table = table.unwrap_or_else(|| Arc::new(CommandTable::loading()));
    ApiResponse::json(200, &view(&table))
}

/// `table` as the browser reads it. Every string came from a session's `initialize` row, so
/// each leaves the server flattened.
fn view(table: &CommandTable) -> CommandsView {
    let (state, generation, reason) = match table.state() {
        CatalogueState::Loading => (CatalogueWireState::Loading, None, None),
        CatalogueState::Ready { generation } => {
            (CatalogueWireState::Ready, Some(*generation), None)
        }
        CatalogueState::Unavailable { reason } => (
            CatalogueWireState::Unavailable,
            None,
            Some(inline_safe(reason)),
        ),
    };
    CommandsView {
        state,
        generation,
        reason,
        entries: table.entries().iter().map(flattened).collect(),
    }
}

fn flattened(entry: &CommandEntry) -> CommandEntry {
    CommandEntry {
        name: inline_safe(&entry.name),
        aliases: entry
            .aliases
            .iter()
            .map(|alias| inline_safe(alias))
            .collect(),
        description: inline_safe(&entry.description),
        argument_hint: inline_safe(&entry.argument_hint),
        source: entry.source.clone(),
        class: entry.class.clone(),
        reason: entry.reason.as_deref().map(inline_safe),
    }
}

/// The frame of `session`'s `table`.
fn frame(session: &str, table: &CommandTable) -> PlanMessage {
    let CommandsView {
        state,
        generation,
        reason,
        entries,
    } = view(table);
    PlanMessage::Commands {
        session: inline_safe(session),
        state,
        generation,
        reason,
        entries,
    }
}

/// Own the chat session's catalogue: a side process per chat session, its tables published to
/// the session's gate and to every tab, the latest frame replayed to a new tab.
pub(crate) fn install(ctx: &Arc<PlanContext>, bridge: &Arc<Bridge>) {
    let catalogue = Arc::new(Catalogue::new());
    ctx.extensions().insert(Arc::clone(&catalogue));
    let requests = owner::listen(bridge);
    owner::spawn(ctx, bridge, &catalogue, requests);
    replay_to_new_tabs(ctx, bridge, &catalogue);
}

fn replay_to_new_tabs(ctx: &Arc<PlanContext>, bridge: &Arc<Bridge>, catalogue: &Arc<Catalogue>) {
    let (bridge, catalogue) = (Arc::downgrade(bridge), Arc::downgrade(catalogue));
    ctx.hub().on_open(Box::new(move |_conn| {
        let (Some(bridge), Some(catalogue)) = (bridge.upgrade(), catalogue.upgrade()) else {
            return Vec::new();
        };
        let id = bridge.chat_session_id();
        let latest = id.and_then(|id| Some((catalogue.table(&id)?, id)));
        latest
            .map(|(table, id)| frame(&id, &table))
            .into_iter()
            .collect()
    }));
}
