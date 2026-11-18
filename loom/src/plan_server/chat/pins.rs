//! Turn pins: the bridge's turn boundaries recorded against the text store to find the files a turn changed.

use std::sync::{Arc, Weak};

use crate::plan_server::context::PlanContext;
use crate::plan_server::files::texts::{TextStore, TurnId};
use crate::plan_session::bridge::TurnPins;

pub struct TextPins {
    store: Arc<TextStore>,
    ctx: Weak<PlanContext>,
}

impl TextPins {
    pub fn new(store: Arc<TextStore>, ctx: Weak<PlanContext>) -> Self {
        Self { store, ctx }
    }
}

impl TurnPins for TextPins {
    fn begin_turn(&self, turn: &str) {
        self.store.begin_turn(&TurnId(turn.into()));
    }

    fn end_turn(&self, turn: &str) -> Vec<(String, Option<String>)> {
        let Some(ctx) = self.ctx.upgrade() else {
            return Vec::new();
        };
        let project = ctx.project();
        let changes = self
            .store
            .end_turn(&TurnId(turn.into()), ctx.root(), &project);
        changes
            .into_iter()
            .map(|change| {
                (
                    change.path.to_string_lossy().into_owned(),
                    change.before_hash,
                )
            })
            .collect()
    }
}
