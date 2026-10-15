//! Contracts for plan-wide token totals per model and effort, the 5 s tracker's
//! pass memo, the span-aware rate and the per-bucket cache-hit series.

#[allow(dead_code)]
#[path = "token_flow_contracts/support.rs"]
mod support;

#[allow(dead_code)]
#[path = "token_insights_contracts/lines.rs"]
mod lines;

use lines::*;
use loom::tokens::{read_flow, BUCKET_SECS, MAX_MODEL_KEYS, STAGE_SERIES_LEN};
use support::*;

const OPUS: &str = "claude-opus-5-5";

#[test]
fn claude_usage_split_by_turn_effort() {
    let fx = Fx::new();
    let now = unix_now();
    let lines = [
        assistant(
            "msg_1",
            now - 90,
            OPUS,
            Some("xhigh"),
            Some("xhigh"),
            &usage(2, 1000, 5000, 100),
        ),
        assistant(
            "msg_2",
            now - 80,
            OPUS,
            Some("xhigh"),
            Some("high"),
            &usage(2, 400, 3000, 50),
        ),
        assistant(
            "msg_3",
            now - 70,
            "<synthetic>",
            None,
            None,
            &usage(0, 0, 0, 0),
        ),
    ];
    fx.add_stage("alpha", &uuid(1), &body(&lines));

    let flow = fx.scan(now);

    let expected = vec![
        model_entry(OPUS, "xhigh", usage(2, 1000, 5000, 100)),
        model_entry(OPUS, "high", usage(2, 400, 3000, 50)),
    ];
    assert_eq!(flow.models, expected, "models split by perTurnEffort");
    assert_eq!(models_sum(&flow.models), flow.plan.total);
}

#[test]
fn codex_usage_carries_turn_context_effort() {
    let fx = Fx::new();
    let now = unix_now();
    let loom_session = fx.add_stage("alpha", &uuid(1), "");
    fx.add_forward_receipt("alpha", &uuid(1), &loom_session, THREAD_ID);
    add_effort_rollout(&fx, THREAD_ID, now - 120, "xhigh", [1000, 600, 50]);

    let flow = fx.scan(now);

    let sol: Vec<_> = flow
        .models
        .iter()
        .filter(|entry| entry.model == SOL_MODEL)
        .cloned()
        .collect();
    assert_eq!(
        sol,
        vec![model_entry(SOL_MODEL, "xhigh", usage(400, 0, 600, 50))],
        "codex usage booked under turn_context's effort: {:?}",
        flow.models
    );
    assert_eq!(models_sum(&flow.models), flow.plan.total);
}

#[test]
fn effort_label_is_sanitized() {
    let fx = Fx::new();
    let now = unix_now();
    let effort: String = std::iter::once('x')
        .chain(BAD)
        .chain("high".chars())
        .collect();
    let line = assistant(
        "msg_1",
        now - 60,
        OPUS,
        None,
        Some(&effort),
        &usage(2, 100, 50, 7),
    );
    fx.add_stage("alpha", &uuid(1), &body(&[line]));

    let scanned = fx.scan(now);
    let written = read_flow(&fx.work_root).expect("flow.json readable after the pass");

    for flow in [&scanned, &written] {
        assert!(!flow.models.is_empty(), "no models entry: {flow:?}");
        for entry in &flow.models {
            let effort = entry.effort.as_deref().expect("effort kept");
            assert!(!effort.is_empty(), "effort emptied: {entry:?}");
            assert_clean("models effort", effort);
            assert_clean("models model", &entry.model);
        }
    }
}

#[test]
fn model_keys_capped_per_agent() {
    let fx = Fx::new();
    let now = unix_now();
    let lines: Vec<String> = (1..=20)
        .map(|n| {
            let model = format!("m-{n:02}");
            let id = format!("msg_{n}");
            assistant(
                &id,
                now - 100 + n,
                &model,
                None,
                Some("xhigh"),
                &usage(0, 10, 0, 0),
            )
        })
        .collect();
    fx.add_stage("alpha", &uuid(1), &body(&lines));

    let flow = fx.scan(now);

    assert!(
        flow.models.len() <= MAX_MODEL_KEYS + 1,
        "{} models entries for one agent: {:?}",
        flow.models.len(),
        flow.models
    );
    let others = flow.models.iter().filter(|e| e.model == "other").count();
    assert_eq!(others, 1, "expected one other entry: {:?}", flow.models);
    assert_eq!(models_sum(&flow.models).cache_write, 200);
    assert_eq!(models_sum(&flow.models), flow.plan.total);
}

#[test]
fn idle_flow_not_rewritten_within_heartbeat() {
    let fx = Fx::new();
    let now = unix_now() / BUCKET_SECS * BUCKET_SECS;
    let line = assistant(
        "msg_1",
        now - 7200,
        OPUS,
        None,
        Some("high"),
        &usage(2, 100, 50, 7),
    );
    fx.add_stage("alpha", &uuid(1), &body(&[line]));
    let observed = |at: i64| {
        fx.scan(at);
        read_flow(&fx.work_root).expect("flow.json").observed_at
    };

    let seen = [
        observed(now),
        observed(now + 5),
        observed(now + 30),
        observed(now + 65),
    ];

    assert_eq!(
        seen,
        [now, now, now, now + 65],
        "observed_at after each pass"
    );
}

#[test]
fn tracker_memo_keeps_work_roots_apart() {
    let (a, b) = (Fx::new(), Fx::new());
    let now = unix_now();
    let line = |cw: u64| claude_line("msg_1", now - 60, &usage(0, cw, 0, 0));
    a.add_stage("alpha", &uuid(1), &body(&[line(100)]));
    b.add_stage("alpha", &uuid(1), &body(&[line(7)]));

    let first_a = a.scan(now).plan.total.cache_write;
    let first_b = b.scan(now).plan.total.cache_write;
    let second_a = a.scan(now + 5).plan.total.cache_write;
    let file = |fx: &Fx| {
        read_flow(&fx.work_root)
            .expect("flow.json")
            .plan
            .total
            .cache_write
    };

    assert_eq!(
        [first_a, first_b, second_a],
        [100, 7, 100],
        "returned flows"
    );
    assert_eq!([file(&a), file(&b)], [100, 7], "flow.json of each root");
}

#[test]
fn rate_steady_across_bucket_boundary() {
    let fx = Fx::new();
    let t = unix_now() / BUCKET_SECS * BUCKET_SECS;
    let lines: Vec<String> = (1..=30)
        .map(|n| claude_line(&format!("msg_{n}"), t - 310 + n * 10, &usage(0, 1000, 0, 0)))
        .collect();
    fx.add_stage("alpha", &uuid(1), &body(&lines));

    let before = stage(&fx.scan(t - 1), "alpha").rate.cache_write;
    let at_t = claude_line("msg_t", t, &usage(0, 1000, 0, 0));
    append(&fx.transcript(&uuid(1)), &format!("{at_t}\n"));
    let after = stage(&fx.scan(t + 1), "alpha").rate.cache_write;

    assert_eq!(before, 6000, "30000 over the full 300 s window");
    let drift = before.abs_diff(after) as f64 / before as f64;
    assert!(
        drift <= 0.05,
        "rate moved {:.1}% across the boundary: {before} then {after}",
        drift * 100.0
    );
}

#[test]
fn hit_series_tracks_bucket_cache_hit() {
    let fx = Fx::new();
    let k = unix_now() / BUCKET_SECS;
    let now = k * BUCKET_SECS + 15;
    let lines = [
        claude_line("msg_1", (k - 2) * BUCKET_SECS + 5, &usage(0, 100, 300, 5)),
        claude_line("msg_2", (k - 1) * BUCKET_SECS + 5, &usage(0, 300, 100, 5)),
    ];
    fx.add_stage("alpha", &uuid(1), &body(&lines));

    let flow = fx.scan(now);
    let hit = &stage(&flow, "alpha").hit_series;

    assert_eq!(hit.len(), STAGE_SERIES_LEN);
    assert_eq!(hit[STAGE_SERIES_LEN - 3..], [Some(75), Some(25), None]);
}
