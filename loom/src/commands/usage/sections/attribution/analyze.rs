//! Per-transcript cost definitions: totals, startup and the admission cost of
//! every tool result, user message and attachment.

use std::collections::{BTreeMap, HashMap};

use crate::commands::usage::accounting::Accounting;
use crate::commands::usage::transcript::{Entry, Request, ToolUse, Transcript};
use crate::commands::usage::transcript_types::SYNTHETIC_MODEL;

use super::classify;

/// Cache-read price relative to fresh input.
const READ_RATE: f64 = 0.1;
/// Output price relative to fresh input.
const OUTPUT_RATE: f64 = 5.0;
/// Characters per token in the estimate.
const CHARS_PER_TOKEN: f64 = 4.0;

pub(super) struct Item {
    pub class: String,
    pub chars: usize,
    pub s2: f64,
}

/// The first request's resident context and the attachment characters per
/// kind that were already in it.
pub(super) struct Startup {
    pub resident: u64,
    pub kind_chars: BTreeMap<String, usize>,
}

pub(super) struct Analysis {
    pub total_s2: f64,
    pub startup_s2: f64,
    pub output_s2: f64,
    pub replay_s2: f64,
    pub items: Vec<Item>,
    /// `Some` only when the first request is provably the session's start.
    pub startup: Option<Startup>,
}

/// Request counts and cache write price for one transcript.
struct Costs {
    requests: usize,
    write_rate: f64,
}

impl Costs {
    fn of(requests: &[&Request]) -> Costs {
        let (mut five, mut hour) = (0.0, 0.0);
        for request in requests {
            five += request.usage.ephemeral_5m as f64;
            hour += request.usage.ephemeral_1h as f64;
        }
        let write_rate = if five + hour == 0.0 {
            1.25
        } else {
            (1.25 * five + 2.0 * hour) / (five + hour)
        };
        Costs {
            requests: requests.len(),
            write_rate,
        }
    }

    /// Cost of `chars` characters admitted after `k` requests: request `k + 1`
    /// writes them and the `n - k - 1` after it read them. Nothing admitted
    /// before the first request (startup) or after the last (never sent) costs.
    fn admission_s2(&self, chars: usize, k: usize) -> Option<f64> {
        if k == 0 || k >= self.requests {
            return None;
        }
        let reads = (self.requests - k - 1) as f64;
        Some(chars as f64 / CHARS_PER_TOKEN * (self.write_rate + READ_RATE * reads))
    }
}

/// `None` when the transcript has no non-synthetic request.
pub(super) fn analyze(transcript: &Transcript) -> Option<Analysis> {
    let requests: Vec<&Request> = transcript
        .requests()
        .filter(|request| request.model != SYNTHETIC_MODEL)
        .collect();
    let first = *requests.first()?;
    let costs = Costs::of(&requests);
    let (items, kind_chars) = admitted(transcript, &costs);
    let fresh = first.normalization.true_fresh_start == Some(true);
    Some(Analysis {
        total_s2: requests
            .iter()
            .map(|request| Accounting::of(&request.usage).s2)
            .sum(),
        startup_s2: if fresh {
            startup_s2(first, &costs)
        } else {
            0.0
        },
        output_s2: requests.iter().map(|r| r.usage.output as f64).sum::<f64>() * OUTPUT_RATE,
        replay_s2: replay_s2(&requests, &costs),
        items,
        startup: fresh.then(|| Startup {
            resident: first.usage.resident(),
            kind_chars,
        }),
    })
}

/// The first request's own cost plus its resident context re-read by every
/// later request.
fn startup_s2(first: &Request, costs: &Costs) -> f64 {
    let usage = &first.usage;
    usage.input as f64
        + READ_RATE * usage.cache_read as f64
        + 1.25 * usage.ephemeral_5m as f64
        + 2.0 * usage.ephemeral_1h as f64
        + READ_RATE * usage.resident() as f64 * (costs.requests - 1) as f64
}

/// Every request's output except the last is written back into the context for
/// the requests after it. An upper bound: thinking may not be replayed.
fn replay_s2(requests: &[&Request], costs: &Costs) -> f64 {
    requests
        .iter()
        .enumerate()
        .take(costs.requests - 1)
        .map(|(index, request)| {
            let reads = (costs.requests - index - 2) as f64;
            request.usage.output as f64 * (costs.write_rate + READ_RATE * reads)
        })
        .sum()
}

/// Costed items after the first request, and the characters per attachment
/// kind that were present before it.
fn admitted(transcript: &Transcript, costs: &Costs) -> (Vec<Item>, BTreeMap<String, usize>) {
    let tools: HashMap<&str, &ToolUse> = transcript
        .requests()
        .flat_map(|request| &request.tool_uses)
        .map(|tool| (tool.id.as_str(), tool))
        .collect();
    let mut items = Vec::new();
    let mut push = |class: String, chars: usize, k: usize| {
        if let Some(s2) = costs.admission_s2(chars, k) {
            items.push(Item { class, chars, s2 });
        }
    };
    let mut seen = 0;
    for entry in &transcript.entries {
        match entry {
            Entry::Assistant(request) => seen += usize::from(request.model != SYNTHETIC_MODEL),
            Entry::User(user) => {
                let class = match &user.tool_use_id {
                    Some(id) => classify::tool_result(tools.get(id.as_str()).copied()),
                    None => "user-text".to_owned(),
                };
                push(class, user.text.chars().count(), seen);
            }
        }
    }
    let mut kind_chars = BTreeMap::new();
    for attachment in &transcript.attachments {
        if attachment.requests_before == 0 {
            *kind_chars.entry(attachment.kind.clone()).or_default() += attachment.chars;
        } else {
            let class = format!("attach:{}", attachment.kind);
            push(class, attachment.chars, attachment.requests_before);
        }
    }
    (items, kind_chars)
}
