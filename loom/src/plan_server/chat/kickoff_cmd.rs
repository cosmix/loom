//! Holder confirmation of the kickoff's one creation grant.

use super::dispatch::{
    bridge, chat_state, checked, refuse_new_work, remember_holder, Outcome, NO_CHAT,
};
use super::lock;
use crate::plan_server::authoring::{Refusal, Take};
use crate::plan_server::context::PlanContext;
use crate::plan_server::ids::ConnCtx;
use crate::plan_server::wire::message::PlanMessage;
use crate::plan_session::bridge::Bridge;
use crate::plan_session::kind::SessionDir;
use crate::plan_session::scope::{self, ArmRefusal, ArmedTarget, KickoffSize};

pub(super) fn confirm(ctx: &PlanContext, conn: &ConnCtx, size: KickoffSize, name: &str) -> Outcome {
    refuse_new_work(ctx)?;
    if ctx.read_only().is_read_only() {
        return Err(ctx.refusal_message(&Refusal::ReadOnlyMode));
    }
    let (bridge, state) = (bridge(ctx)?, chat_state(ctx)?);
    // Scope arming is a read/replace operation: serialize competing confirmations.
    let _starting = state.starting();
    ctx.require_authoring(&conn.browser, Take::HolderOnly)
        .map_err(|refusal| ctx.refusal_message(&refusal))?;
    remember_holder(ctx, conn);
    let id = bridge.chat_session_id().ok_or_else(|| NO_CHAT.to_owned())?;
    let dir = SessionDir::new(ctx.runtime().sessions.join(&id));
    let target = arm_and_send(ctx, &bridge, &dir, &id, size, name)?;
    let document = target.document.to_string_lossy();
    let view = lock(&state.kickoff).confirm(&id, &document);
    if let Some(view) = view {
        ctx.hub().broadcast(&PlanMessage::Kickoff(Box::new(view)));
    }
    Ok(Vec::new())
}

fn arm_and_send(
    ctx: &PlanContext,
    bridge: &Bridge,
    dir: &SessionDir,
    id: &str,
    size: KickoffSize,
    name: &str,
) -> Result<ArmedTarget, String> {
    let message = checked(ctx, &confirmation(size, name), Vec::new())?;
    let previous = scope::load(dir)
        .map_err(|error| ArmRefusal::Unreadable(format!("{error:#}")).to_string())?;
    let target =
        scope::arm_kickoff(dir, ctx.root(), size, name).map_err(|refusal| refusal.to_string())?;
    if let Err(error) = bridge.send(id, message.formatted, message.attachments) {
        if let Err(rollback) = scope::write(dir, &previous) {
            tracing::error!(session = %id, %rollback, "cannot roll back kickoff confirmation");
        }
        return Err(error.to_string());
    }
    Ok(target)
}

fn confirmation(size: KickoffSize, name: &str) -> String {
    let target = match size {
        KickoffSize::Feature => ArmedTarget {
            document: format!("doc/product/{name}/index.md").into(),
            bundle: None,
        },
        KickoffSize::Plan => ArmedTarget {
            document: format!("doc/plans/PLAN-{name}.md").into(),
            bundle: Some(format!("doc/plans/briefs/{name}").into()),
        },
    };
    let size = match size {
        KickoffSize::Feature => "feature",
        KickoffSize::Plan => "standalone plan",
    };
    let mut text = format!(
        "The user confirmed the {size} \"{name}\". You may now create {}",
        target.document.display()
    );
    if let Some(bundle) = &target.bundle {
        text.push_str(&format!(" and files inside {}/", bundle.display()));
    }
    text
}
