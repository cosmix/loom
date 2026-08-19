//! `loom hook user-prompt` — the deterministic UserPromptSubmit entry point.
//!
//! Reads the hook payload from stdin, retrieves a small pack for the prompt,
//! and prints one JSON object carrying the units this recipient has not already
//! been handed in this epoch. There is **no model call and no network call**
//! anywhere below this function: retrieval is pure filesystem and string work.
//!
//! A loom stage is not a precondition. Inside one the brief is keyed to the
//! stage; outside one it is keyed to the checkout's working-tree overlay — the
//! same `(plan, stage)` address `loom map` and `loom knowledge sync` write — so
//! an ordinary session in a mapped repository gets the same retrieval a stage
//! does.
//!
//! Every failure is an empty stdout and exit 0. A prompt-submit hook that
//! reports a problem is a prompt-submit hook that interrupts the session, so
//! the only two outcomes here are "one useful object" and "nothing at all".

use crate::context::delivery::{self, DeliveryRecord};
use crate::context::local_overlay::local_overlay_key;
use crate::context::retrieve::{context_epoch, retrieve_for_stage, StageQuery};
use crate::context::schema::{ContextItem, ContextPack};
use crate::fs::work_dir::WorkDir;
use crate::validation::validate_id;
use anyhow::Result;
use std::collections::BTreeSet;
use std::io::Read;
use std::path::PathBuf;

/// Estimated tokens one prompt's brief may be packed against.
///
/// It bounds how much retrieval is worth doing for a single question. Token
/// counts throughout this path are estimates derived from byte length, never a
/// measurement of what any model receives.
const PROMPT_BUDGET_TOKENS: usize = 1500;

/// Longest stdin payload worth parsing. Anything past it is malformed by
/// definition, which keeps a runaway writer from being read into memory.
const MAX_STDIN_BYTES: u64 = 1024 * 1024;

/// Shortest prompt worth retrieving against — a bare acknowledgement asks
/// nothing, so it earns nothing.
const MIN_PROMPT_CHARS: usize = 24;

/// Hard ceiling on the serialized hook object.
const MAX_PAYLOAD_BYTES: usize = 8 * 1024;

/// What the shared renderer reports on its `Selected from:` line. A prompt hook
/// retrieves against the one question that was just typed, not against the
/// stage's whole query surface.
const QUERY_INPUTS: &str = "this prompt";

/// Where a prompt-hook delivery is filed, and what it is filed against.
struct DeliveryTarget {
    /// The `.work/` root a delivery record is filed under. It may not exist:
    /// a session outside any loom project still retrieves, it just has nowhere
    /// to record what it was handed.
    work_dir: PathBuf,
    /// Directory retrieval resolves the knowledge tree, the context cache and
    /// the overlay address from — the parent of [`Self::work_dir`].
    project_root: PathBuf,
    plan: String,
    stage_id: String,
}

/// One prompt's brief: the line to print, and what filing it commits to.
struct Emission {
    target: DeliveryTarget,
    payload: String,
    handed_over: ContextPack,
}

/// Emit a retrieval brief for the prompt on stdin, or emit nothing.
pub fn user_prompt() -> Result<()> {
    let Some(prompt) = read_prompt() else {
        return Ok(());
    };
    let Some(emission) = retrieve_for_prompt(prompt) else {
        return Ok(());
    };

    // Printed BEFORE the record is filed, and deliberately so: a delivery
    // recorded but never printed is a gap — those units stay suppressed for the
    // rest of the epoch without the session ever having seen them — while a
    // print never recorded costs one repeat delivery.
    println!("{}", emission.payload);
    emission.target.record(&emission.handed_over);
    Ok(())
}

/// Everything between reading stdin and writing stdout: resolve the recipient,
/// retrieve, and drop what it already holds. `None` wherever there is nothing
/// honest to say.
fn retrieve_for_prompt(prompt: String) -> Option<Emission> {
    let target = DeliveryTarget::from_environment()?;

    let query = StageQuery::new(&target.project_root, prompt);
    let pack = match retrieve_for_stage(&query, PROMPT_BUDGET_TOKENS) {
        Ok(pack) => pack,
        Err(error) => {
            tracing::debug!(%error, "No retrieval available for this prompt");
            return None;
        }
    };

    let delivered = target.already_delivered(&pack);
    let (payload, handed_over) = compose(&target.stage_id, &pack, &delivered)?;
    Some(Emission {
        target,
        payload,
        handed_over,
    })
}

impl DeliveryTarget {
    /// The stage this session is executing, or the checkout it is sitting in.
    ///
    /// A stage target is preferred whenever the environment really names one:
    /// its delivery record is the one the spawn brief already wrote to, so
    /// falling back to the local target there would re-deliver everything that
    /// brief carried.
    fn from_environment() -> Option<Self> {
        Self::for_stage().or_else(Self::for_checkout)
    }

    /// Resolve from `LOOM_STAGE_ID` and `LOOM_WORK_DIR`. `None` when either is
    /// unset or unusable, or when the id names no stage on disk — all of which
    /// mean there is no stage to deliver anything to, and the caller falls back
    /// to [`Self::for_checkout`].
    ///
    /// The stage id arrives from the environment and becomes a path component,
    /// so it is validated HERE, at the boundary, exactly as
    /// `commands::context::record_edit` validates its own `--stage`. Relying on
    /// `record_delivery`'s recipient check to catch a traversal would be relying
    /// on an accident: that check runs against a *different* string, the
    /// composed `prompt-<id>` key, and changing the key's shape would open the
    /// traversal back up silently.
    ///
    /// The plan component comes from the stage record rather than from
    /// `config.toml`, because the spawn path keys its own delivery record on
    /// exactly that (`delivery::plan_key`). Reading a different derivation here
    /// would look in a directory the writer never wrote to, and re-deliver
    /// everything the spawn brief already carried.
    fn for_stage() -> Option<Self> {
        let stage_id = non_empty_env("LOOM_STAGE_ID")?;
        validate_id(&stage_id).ok()?;
        let work_dir = WorkDir::new(non_empty_env("LOOM_WORK_DIR")?).ok()?;
        let stage = crate::verify::load_stage(&stage_id, work_dir.root()).ok()?;
        Some(DeliveryTarget {
            project_root: work_dir.project_root()?.to_path_buf(),
            work_dir: work_dir.root().to_path_buf(),
            plan: delivery::plan_key(&stage).to_string(),
            stage_id,
        })
    }

    /// The checkout this session is running in, for a prompt that no stage
    /// claims — an ordinary Claude Code session in a mapped repository.
    ///
    /// The address is [`local_overlay_key`]'s, so this reads exactly the
    /// working-tree overlay `loom map` and `loom knowledge sync` write, which is
    /// also what [`StageQuery::new`]'s default `OverlayScope::Local` resolves
    /// to. Deriving a second address here would read an overlay nothing writes.
    ///
    /// No `validate_id` call guards the derived stage name, unlike
    /// [`Self::for_stage`]: this one is not environment input. It is
    /// `map-<canonical directory name>`, a single path component by
    /// construction — a `file_name` can hold no separator, and the `map-` prefix
    /// means it can never be `.` or `..`. Its recipient key is still checked by
    /// `delivery::record_delivery`, which refuses anything outside
    /// `[A-Za-z0-9._-]` and so simply files nothing for a directory named with
    /// something exotic.
    fn for_checkout() -> Option<Self> {
        let hint = non_empty_env("LOOM_WORK_DIR").unwrap_or_else(|| ".".to_string());
        let work_dir = WorkDir::new(hint).ok()?;
        let project_root = work_dir.project_root()?.to_path_buf();
        let (plan, stage_id) = local_overlay_key(&project_root);
        Some(DeliveryTarget {
            work_dir: work_dir.root().to_path_buf(),
            project_root,
            plan,
            stage_id,
        })
    }

    /// The `(node_id, content_hash)` pairs this recipient already holds under the
    /// pack's epoch. Unreadable records read as "nothing delivered": a repeat
    /// delivery is a waste, a missing one is a gap.
    fn already_delivered(&self, pack: &ContextPack) -> BTreeSet<(String, String)> {
        let epoch = context_epoch(pack);
        match delivery::load_deliveries(&self.work_dir, &self.plan, &self.stage_id) {
            Ok(records) => delivery::delivered_in_epoch(&records, &epoch),
            Err(error) => {
                tracing::debug!(%error, "Unreadable delivery records; assuming none");
                BTreeSet::new()
            }
        }
    }

    /// File what was just handed over so the same units stay suppressed for the
    /// rest of the epoch.
    ///
    /// The recipient key is stable per stage, so repeated prompts in one epoch
    /// converge on a single record instead of accumulating one file per
    /// question. What makes that safe is that [`delivery::record_delivery`]
    /// folds a delivery into the record already there rather than replacing it.
    /// A failure costs a repeat delivery and nothing else, so it is logged and
    /// ignored — the hook has already printed.
    ///
    /// With no `.work/` there is nowhere to file anything, and that is not a
    /// failure either: a prompt hook running in a project that never asked for
    /// loom must not create a `.work/` tree as a side effect of answering one
    /// question. The cost is that suppression cannot work there — every prompt
    /// re-delivers what the last one was given — which is the same "nothing
    /// delivered" the unreadable-record path already accepts.
    fn record(&self, handed_over: &ContextPack) {
        if !self.work_dir.exists() {
            tracing::debug!("No .work/ to file a prompt-hook delivery record in");
            return;
        }
        let recipient = format!("prompt-{}", self.stage_id);
        let record = DeliveryRecord::from_pack(recipient, handed_over);
        if let Err(error) =
            delivery::record_delivery(&self.work_dir, &self.plan, &self.stage_id, &record)
        {
            tracing::debug!(%error, "Could not file a prompt-hook delivery record");
        }
    }
}

/// A set environment variable with non-blank content.
fn non_empty_env(name: &str) -> Option<String> {
    let value = std::env::var(name).ok()?;
    (!value.trim().is_empty()).then_some(value)
}

/// The prompt from the hook payload on stdin.
///
/// The shell side owns the timeout; the byte cap here is what keeps a
/// pathological payload bounded.
fn read_prompt() -> Option<String> {
    let mut raw = String::new();
    std::io::stdin()
        .lock()
        .take(MAX_STDIN_BYTES)
        .read_to_string(&mut raw)
        .ok()?;
    parse_prompt(&raw)
}

/// Extract a usable prompt from a hook payload. `None` for empty or malformed
/// JSON, a missing `prompt` field, or a prompt too short to have asked anything.
fn parse_prompt(raw: &str) -> Option<String> {
    let payload: serde_json::Value = serde_json::from_str(raw).ok()?;
    let prompt = payload.get("prompt")?.as_str()?.trim();
    (prompt.chars().count() >= MIN_PROMPT_CHARS).then(|| prompt.to_string())
}

/// Compose the hook's single stdout line, together with the pack that line
/// actually delivers.
///
/// The brief itself is rendered by
/// [`crate::orchestrator::signals::format_knowledge_brief`] — the same renderer
/// the signal path uses. Its fencing rule is what keeps an untrusted excerpt
/// from escaping its own quoted block, and a containment rule with two copies
/// is a containment rule that drifts, so this path never forks it. Only the
/// budget, the per-epoch dedupe, and the serialized ceiling are the hook's own.
///
/// An over-budget pack DEGRADES rather than being discarded: its weakest units
/// are dropped until the object fits. Discarding instead would recompose and
/// throw away the same pack on every prompt for the rest of the epoch, and would
/// file no record — so the strongest matches, which do fit, would never arrive.
///
/// `None` — emit nothing — only where there is no honest payload: a pack whose
/// every unit this epoch already delivered, or a single unit that is over the
/// ceiling by itself and so cannot be trimmed into fitting.
fn compose(
    stage_id: &str,
    pack: &ContextPack,
    delivered: &BTreeSet<(String, String)>,
) -> Option<(String, ContextPack)> {
    let mut handed_over = undelivered(pack, delivered)?;
    loop {
        let line = render_payload(stage_id, &handed_over)?;
        if line.len() <= MAX_PAYLOAD_BYTES {
            return Some((line, handed_over));
        }
        handed_over = without_weakest(&handed_over)?;
    }
}

/// The single stdout line for `handed_over`: the shared brief wrapped in the
/// hook's JSON envelope.
fn render_payload(stage_id: &str, handed_over: &ContextPack) -> Option<String> {
    let brief =
        crate::orchestrator::signals::format_knowledge_brief(handed_over, stage_id, QUERY_INPUTS);
    let payload = serde_json::json!({
        "hookSpecificOutput": {
            "hookEventName": "UserPromptSubmit",
            "additionalContext": brief,
        }
    });
    serde_json::to_string(&payload).ok()
}

/// `pack` minus every unit already delivered in this epoch, or `None` when
/// nothing survives.
fn undelivered(pack: &ContextPack, delivered: &BTreeSet<(String, String)>) -> Option<ContextPack> {
    let items: Vec<ContextItem> = pack
        .items
        .iter()
        .filter(|item| {
            let key = (item.id.as_str().to_string(), item.content_hash.clone());
            !delivered.contains(&key)
        })
        .cloned()
        .collect();
    if items.is_empty() {
        return None;
    }
    Some(carrying(pack, items))
}

/// `pack` without its lowest-scoring unit, or `None` when a single unit is all
/// that is left — one unit that does not fit cannot be trimmed into fitting.
fn without_weakest(pack: &ContextPack) -> Option<ContextPack> {
    if pack.items.len() <= 1 {
        return None;
    }
    let weakest = (0..pack.items.len()).min_by(|&left, &right| {
        pack.items[left]
            .score
            .total_cmp(&pack.items[right].score)
            // Ties drop the later unit: pack order is strongest first.
            .then(right.cmp(&left))
    })?;

    let mut items = pack.items.clone();
    items.remove(weakest);
    let mut narrowed = carrying(pack, items);
    // A unit dropped for size is a ranked candidate that did not fit, which is
    // exactly what the brief's "Omitted: N weaker matches" line reports. Left
    // alone it would tell the reader it had been given everything.
    narrowed.omitted.omitted += 1;
    narrowed.omitted.weakest_included_score = narrowed
        .items
        .iter()
        .map(|item| item.score)
        .fold(f32::INFINITY, f32::min);
    Some(narrowed)
}

/// `pack` carrying exactly `items`, with the token estimate that describes them.
fn carrying(pack: &ContextPack, items: Vec<ContextItem>) -> ContextPack {
    let mut narrowed = pack.clone();
    narrowed.estimated_tokens = items.iter().map(|item| item.token_count).sum();
    narrowed.items = items;
    narrowed
}

#[cfg(test)]
#[path = "tests_user_prompt.rs"]
mod tests;
