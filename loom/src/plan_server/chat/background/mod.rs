//! The plan chat's background-task frame: the bridge's table joined with the planner token
//! pass, flattened, broadcast when it changes and replayed to a new tab.

mod groups;
mod state;
#[cfg(test)]
mod tests;

#[cfg(test)]
mod contract_tests;

use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use self::state::Board;
use crate::context::untrusted::inline_safe;
use crate::plan_server::context::PlanContext;
use crate::plan_server::wire::background::BackgroundTaskView;
use crate::plan_server::wire::message::PlanMessage;
use crate::plan_session::bridge::background::BackgroundTask;
use crate::plan_session::bridge::{Bridge, BridgeEvent};
use crate::tokens::{spawn_planner_tokens, AgentFlow, TokenUsage};

/// The frame of `session`'s `tasks`, in the given order. Each task takes the figures of the
/// flow row with its id; every string leaves the server flattened (transcript text is untrusted).
pub(crate) fn frame(session: &str, tasks: &[BackgroundTask], flows: &[AgentFlow]) -> PlanMessage {
    let view = |task: &BackgroundTask| {
        let flow = flows.iter().find(|flow| flow.id == task.id);
        view(task, flow)
    };
    PlanMessage::Background {
        session: inline_safe(session),
        tasks: tasks.iter().map(view).collect(),
    }
}

fn view(task: &BackgroundTask, flow: Option<&AgentFlow>) -> BackgroundTaskView {
    let safe = |text: &Option<String>| text.as_deref().map(inline_safe);
    BackgroundTaskView {
        id: inline_safe(&task.id),
        kind: task.kind,
        label: safe(&task.label),
        agent_type: safe(&task.agent_type),
        tool_use_id: safe(&task.tool_use_id),
        state: task.state,
        started_at: task.started_at,
        last_activity_at: task.last_activity_at,
        steps: task.steps,
        last_tool: safe(&task.last_tool),
        report_preview: safe(&task.report_preview),
        model: flow.and_then(|flow| safe(&flow.model)),
        total: flow.map(|flow| TokenUsage::fresh(&flow.total)),
        tokens_per_min: flow.map(|flow| TokenUsage::fresh(&flow.rate)),
        active: flow.map(|flow| flow.active),
    }
}

/// Publish the chat's background frames and run the planner token pass behind them.
pub(super) fn install(ctx: &Arc<PlanContext>, bridge: &Arc<Bridge>) {
    let board = Arc::new(Board::default());
    // The context owns the board; the listener, the hook and the thread hold it weakly.
    ctx.extensions().insert(Arc::clone(&board));
    follow_bridge(ctx, bridge, &board);
    replay_to_new_tabs(ctx, &board);
    spawn_tokens(ctx, bridge, &board);
}

/// A bridge listener runs after the bridge's lock is released, so `chat_session_id` is safe
/// here; it must never wait on the bridge for anything else.
fn follow_bridge(ctx: &Arc<PlanContext>, bridge: &Arc<Bridge>, board: &Arc<Board>) {
    let (weak, bridge_weak, board) = (
        Arc::downgrade(ctx),
        Arc::downgrade(bridge),
        Arc::downgrade(board),
    );
    bridge.subscribe(Box::new(move |event| {
        let (Some(ctx), Some(bridge), Some(board)) =
            (weak.upgrade(), bridge_weak.upgrade(), board.upgrade())
        else {
            return;
        };
        let emit = |message: &PlanMessage| ctx.hub().broadcast(message);
        match event {
            BridgeEvent::Background { id, tasks } => {
                if bridge.chat_session_id().as_deref() == Some(id.as_str()) {
                    board.tasks(id, tasks.clone(), &emit);
                }
            }
            BridgeEvent::Ended { id, .. } => board.ended(id, &emit),
            _ => {}
        }
    }));
}

fn replay_to_new_tabs(ctx: &Arc<PlanContext>, board: &Arc<Board>) {
    let board = Arc::downgrade(board);
    ctx.hub().on_open(Box::new(move |_conn| {
        let last = board.upgrade().and_then(|board| board.replay());
        last.into_iter().collect()
    }));
}

/// The `planner-tokens` thread: it reads the chat session's transcripts every pass and hands
/// the rows to the board. Its closures hold weak references, so it ends on its own when the
/// context and bridge drop; the context also stops and joins it at shutdown.
fn spawn_tokens(ctx: &Arc<PlanContext>, bridge: &Arc<Bridge>, board: &Arc<Board>) {
    let stop = Arc::new(AtomicBool::new(false));
    let dir = ctx.runtime().dir.join("tokens");
    let (bridge_weak, ctx_weak, board_weak) = (
        Arc::downgrade(bridge),
        Arc::downgrade(ctx),
        Arc::downgrade(board),
    );
    let groups = Box::new(move || match bridge_weak.upgrade() {
        Some(bridge) => groups::chat_groups(&bridge),
        None => Vec::new(),
    });
    let sink = Box::new(move |passes| {
        if let (Some(ctx), Some(board)) = (ctx_weak.upgrade(), board_weak.upgrade()) {
            board.flows(passes, &|message| ctx.hub().broadcast(message));
        }
    });
    match spawn_planner_tokens(dir, groups, sink, Arc::clone(&stop)) {
        Ok(handle) => ctx.own_worker("planner tokens", handle, Some(stop)),
        Err(error) => tracing::warn!("chat background tasks show no token figures: {error}"),
    }
}
