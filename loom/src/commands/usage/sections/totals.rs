//! Shows the token components that policy can actually influence: prompt
//! freshness, cache writes, cache reads, and generated output. Thinking is
//! reported as text volume because transcript usage does not measure it.

use crate::commands::usage::transcript::{TokenUsage, Transcript};

use super::fmt::{format_u64, heading, no_data, row};

#[derive(Debug, serde::Serialize)]
pub struct Totals {
    pub requests: usize,
    pub transcripts: usize,
    pub fresh_input: u64,
    pub cache_creation: u64,
    pub cache_reads: u64,
    pub output: u64,
    pub cache_writes_5m: u64,
    pub cache_writes_1h: u64,
    pub thinking_chars: usize,
    pub thinking_tokens_estimate: u64,
}

pub fn build(transcripts: &[Transcript]) -> Totals {
    let mut usage = TokenUsage::default();
    let mut requests = 0;
    let mut thinking_chars = 0;
    for transcript in transcripts {
        for request in transcript.requests() {
            requests += 1;
            thinking_chars += request.thinking_chars;
            usage.add(&request.usage);
        }
    }
    Totals {
        requests,
        transcripts: transcripts.len(),
        fresh_input: usage.input,
        cache_creation: usage.cache_creation,
        cache_reads: usage.cache_read,
        output: usage.output,
        cache_writes_5m: usage.ephemeral_5m,
        cache_writes_1h: usage.ephemeral_1h,
        thinking_chars,
        thinking_tokens_estimate: (thinking_chars as u64) / 4,
    }
}

pub fn render(totals: &Totals) {
    heading("Totals (tokens)");
    if totals.requests == 0 {
        no_data("requests");
        return;
    }
    row("requests", totals.requests);
    row("transcripts", totals.transcripts);
    row("fresh input", format_u64(totals.fresh_input));
    row("cache writes (5m)", format_u64(totals.cache_writes_5m));
    row("cache writes (1h)", format_u64(totals.cache_writes_1h));
    row("cache creation", format_u64(totals.cache_creation));
    row("cache reads", format_u64(totals.cache_reads));
    row("output", format_u64(totals.output));
    row(
        "thinking blocks (chars)",
        format_u64(totals.thinking_chars as u64),
    );
    row(
        "thinking ~tokens (chars/4 estimate)",
        format_u64(totals.thinking_tokens_estimate),
    );
}
