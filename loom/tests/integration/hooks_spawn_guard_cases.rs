use super::*;

#[test]
fn gated_untyped_or_placeholder_spawn_denied() {
    if skip_unless_gate_visible("cases::gated_untyped_or_placeholder_spawn_denied") {
        return;
    }
    let (_temp, hook) = setup_hook();
    let (home, cwd, work) = (temp(), temp(), temp());

    for subagent_type in [None, Some("general-purpose"), Some("claude"), Some("Plan")] {
        let tool_input = match subagent_type {
            Some(t) => json!({"subagent_type": t}),
            None => json!({}),
        };
        let out = gated_task(&hook, tool_input, cwd.path(), home.path(), work.path());

        assert_eq!(
            out.code, 2,
            "subagent_type={subagent_type:?} stderr={}",
            out.stderr
        );
        assert!(
            out.stderr.contains(UNTYPED_MSG_LEAD),
            "subagent_type={subagent_type:?} stderr={}",
            out.stderr
        );
    }
}

#[test]
fn ungated_untyped_spawn_warns_instead_of_blocking() {
    let (_temp, hook) = setup_hook();
    let (home, cwd) = (temp(), temp());

    let out = run_hook(&hook, "Task", json!({}), cwd.path(), home.path(), &[]);

    assert_eq!(out.code, 0, "stderr={}", out.stderr);
    let v: Value = serde_json::from_str(out.stdout.trim()).expect("parse stdout json");
    let ctx = get_str(&v, &["hookSpecificOutput", "additionalContext"])
        .expect("additionalContext present");
    assert!(ctx.starts_with("LOOM_HOOK_WARN:"), "ctx={ctx}");

    // An explicit model inherits nothing, so the message must not claim it does.
    let with_model = json!({"subagent_type": "general-purpose", "model": "haiku"});
    let out = run_hook(&hook, "Task", with_model, cwd.path(), home.path(), &[]);
    let v: Value = serde_json::from_str(out.stdout.trim()).expect("parse stdout json");
    let ctx = get_str(&v, &["hookSpecificOutput", "additionalContext"]).expect("context");
    assert!(
        ctx.contains("Generic agent type 'general-purpose' spawned with explicit model 'haiku'")
            && ctx.contains("loom-software-engineer (sonnet, default)")
            && !ctx.contains(UNTYPED_MSG_LEAD),
        "ctx={ctx}"
    );
}

#[test]
fn typed_spawn_without_model_fills_in_defined_tier_and_preserves_other_keys() {
    let (_temp, hook) = setup_hook();
    let (home, cwd, work) = (temp(), temp(), temp());
    write_agent_def(cwd.path(), "loom-software-engineer", "sonnet");

    let prompt = format!("{PREAMBLE_LINE}\ndo the xyz123 task");
    let description = "xyz123-description";
    let tool_input = json!({
        "subagent_type": "loom-software-engineer",
        "description": description,
        "prompt": prompt,
    });
    let out = gated_task(&hook, tool_input, cwd.path(), home.path(), work.path());

    assert_eq!(out.code, 0, "stderr={}", out.stderr);
    let v: Value = serde_json::from_str(out.stdout.trim()).expect("parse stdout json");
    assert_eq!(
        get_str(&v, &["hookSpecificOutput", "permissionDecision"]),
        Some("allow")
    );
    assert_eq!(
        get_str(&v, &["hookSpecificOutput", "updatedInput", "model"]),
        Some("sonnet")
    );
    assert_eq!(
        get_str(&v, &["hookSpecificOutput", "updatedInput", "description"]),
        Some(description)
    );
    assert_eq!(
        get_str(&v, &["hookSpecificOutput", "updatedInput", "prompt"]),
        Some(prompt.as_str())
    );
}

#[test]
fn explicit_model_escalation_only_warns_above_defined_tier() {
    let (_temp, hook) = setup_hook();
    let (home, cwd, work) = (temp(), temp(), temp());
    write_agent_def(cwd.path(), "loom-senior-software-engineer", "opus");
    write_agent_def(cwd.path(), "loom-software-engineer", "sonnet");
    let prompt = format!("{PREAMBLE_LINE}\ndo the task");

    let at_tier = json!({
        "subagent_type": "loom-senior-software-engineer", "model": "opus", "prompt": prompt,
    });
    let out = gated_task(&hook, at_tier, cwd.path(), home.path(), work.path());
    assert_eq!(out.code, 0, "stderr={}", out.stderr);
    assert!(
        !out.stdout.contains("LOOM_HOOK_WARN") && !out.stdout.contains("updatedInput"),
        "explicit model matching the defined tier must neither warn nor rewrite: stdout={}",
        out.stdout
    );

    let escalated = json!({
        "subagent_type": "loom-software-engineer", "model": "opus", "prompt": prompt,
    });
    let out = gated_task(&hook, escalated, cwd.path(), home.path(), work.path());
    assert_eq!(out.code, 0, "stderr={}", out.stderr);
    let v: Value = serde_json::from_str(out.stdout.trim()).expect("parse stdout json");
    let ctx = get_str(&v, &["hookSpecificOutput", "additionalContext"])
        .expect("additionalContext present");
    assert!(
        ctx.contains("LOOM_HOOK_WARN:") && ctx.contains("escalation"),
        "ctx={ctx}"
    );
}

#[test]
fn spawn_ledger_records_ordered_fields() {
    let (_temp, hook) = setup_hook();
    let (home, cwd) = (temp(), temp());
    write_agent_def(cwd.path(), "loom-software-engineer", "sonnet");
    let prompt = format!("{PREAMBLE_LINE}\ndo the ledger task");
    let description = "ledger-description-xyz";

    let work_a = temp();
    let tool_input_a = json!({
        "subagent_type": "loom-software-engineer", "description": description, "prompt": prompt,
    });
    let out_a = gated_task(&hook, tool_input_a, cwd.path(), home.path(), work_a.path());
    assert_eq!(out_a.code, 0, "stderr={}", out_a.stderr);

    let line_a = read_spawn_line(work_a.path());
    assert_spawn_key_order(&line_a);
    let v_a: Value = serde_json::from_str(&line_a).expect("parse spawn line a");
    assert_eq!(v_a["stage_id"], "test-stage");
    assert_eq!(v_a["caller"], "main");
    assert_eq!(v_a["agent_type"], "loom-software-engineer");
    assert_eq!(v_a["model"], "sonnet");
    assert_eq!(v_a["model_source"], "definition");
    assert_eq!(v_a["description"], description);
    assert!(v_a["ts"].as_str().is_some_and(|s| !s.is_empty()));

    let work_b = temp();
    let tool_input_b = json!({
        "subagent_type": "loom-software-engineer", "model": "sonnet",
        "description": description, "prompt": prompt,
    });
    let out_b = gated_task(&hook, tool_input_b, cwd.path(), home.path(), work_b.path());
    assert_eq!(out_b.code, 0, "stderr={}", out_b.stderr);

    let v_b: Value = serde_json::from_str(&read_spawn_line(work_b.path())).expect("parse line b");
    assert_eq!(v_b["model_source"], "explicit");
}

#[test]
fn non_spawn_tool_is_ignored() {
    let (_temp, hook) = setup_hook();
    let (home, cwd) = (temp(), temp());

    let out = run_hook(
        &hook,
        "Bash",
        json!({"command": "echo hi"}),
        cwd.path(),
        home.path(),
        &[],
    );

    assert_eq!(out.code, 0, "stderr={}", out.stderr);
    assert!(out.stdout.trim().is_empty(), "stdout={}", out.stdout);
}

/// No `LOOM_*` variable is set: the prepend works in every session.
#[test]
fn prompt_without_preamble_gains_it_once_in_the_same_input_as_the_model() {
    let (_temp, hook) = setup_hook();
    let (home, cwd) = (temp(), temp());
    write_agent_def(cwd.path(), "loom-software-engineer", "sonnet");
    let prompt = "  keep: ' \" \\ $(x) `y` λ\n\ttab\n\n";
    let input = json!({
        "subagent_type": "loom-software-engineer", "description": "d", "prompt": prompt,
    });

    let out = run_hook(&hook, "Agent", input, cwd.path(), home.path(), &[]);

    assert_eq!(out.code, 0, "stderr={}", out.stderr);
    assert_eq!(out.stdout.lines().count(), 1, "stdout={}", out.stdout);
    let v: Value = serde_json::from_str(out.stdout.trim()).expect("parse stdout json");
    let updated = &v["hookSpecificOutput"]["updatedInput"];
    assert_eq!(v["hookSpecificOutput"]["permissionDecision"], "allow");
    assert_eq!(updated["model"], "sonnet");
    assert_eq!(updated["description"], "d");
    let rewritten = updated["prompt"].as_str().expect("rewritten prompt");
    assert_eq!(rewritten, with_preamble(prompt));
    assert_eq!(rewritten.matches(PREAMBLE_LINE).count(), 1);

    // A prompt that already carries the line keeps it once, unchanged.
    let again = json!({"subagent_type": "loom-software-engineer", "prompt": rewritten});
    let out = run_hook(&hook, "Agent", again, cwd.path(), home.path(), &[]);
    let v: Value = serde_json::from_str(out.stdout.trim()).expect("parse stdout json");
    assert_eq!(v["hookSpecificOutput"]["updatedInput"]["prompt"], rewritten);
}
