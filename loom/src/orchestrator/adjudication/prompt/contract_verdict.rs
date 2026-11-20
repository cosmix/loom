//! The verdict half of a contract dispute briefing: how the judge rules on
//! each disputed contract, and the JSON it writes.

use super::FROZEN_SET_HEADING;

/// The verdict semantics. `has_replacement` adds what a replacement means for
/// an accept, which no briefing without one mentions.
pub(super) fn semantics(has_replacement: bool) -> String {
    let mut s = String::from("## Verdict semantics\n\n");
    s.push_str("Rule on each disputed contract, one entry of `rulings` apiece.\n\n");
    s.push_str(
        "- accept: the contract's replacement, or the `plan_patch` you give for it, still \
         sets up its `scenario` and still fails on its `rejects` implementation.\n",
    );
    s.push_str(
        "  An accept must change something: the replacement of the contract's file or of a \
         harness file is written, or your `plan_patch` targets the contract. An accept \
         whose file is held back because another contract in it is rejected changes \
         nothing and is refused when you record it, as is one whose only change is a \
         harness file a reject holds back; correct it and record again.\n",
    );
    s.push_str(
        "  A failure caused by where completion runs (no network, no nested OS sandbox, a \
         write outside the tree) is no reason to accept as is: accept a replacement that \
         tests the same scenario in a form that runs there, or, when none can exist, \
         delete the contract with a `plan_patch` and say what it leaves untested.\n",
    );
    s.push_str(&format!(
        "  An accept re-freezes every contract and harness file at its current content, \
         adds new harness files and drops deleted ones, so judge the changes and new \
         files listed under `{FROZEN_SET_HEADING}` too.\n"
    ));
    if has_replacement {
        s.push_str(
            "  An accepted replacement is written into the worktree before the re-freeze. A \
             file holding several contracts is written only when all of them are accepted. A \
             harness file (one holding no contract) is written only when every ruling is \
             accept. Do not \
             both accept a replacement of a contract's file and delete that contract with \
             `plan_patch`: recording refuses it.\n",
        );
    }
    s.push_str(
        "- reject: the change would weaken the contract's scenario or rejects clause, or the \
         contract is right as frozen.",
    );
    if has_replacement {
        s.push_str(" A rejected replacement is discarded and the worktree stays as it is.");
    }
    s.push('\n');
    s.push_str("- needs-more-evidence: you cannot decide; list specific questions.\n\n");
    s.push_str(
        "Your own run of a contract that starts an OS sandbox proves nothing about the \
         contract when your session is itself sandboxed.\n\n",
    );
    s
}

/// The sentence that tells the judge what a citation may point into.
pub(super) fn citation_rule(has_replacement: bool) -> String {
    let replacement = if has_replacement {
        "proposed replacement, "
    } else {
        ""
    };
    format!(
        "Accept and reject require reasoning and citations to real lines in the\n\
         contract, frozen file, {replacement}or diff. A citation has file, optional\n\
         line, excerpt, and claim. Include the command, exit code, and decisive\n\
         output from your test run in a citation when you can run it.\n\n"
    )
}

/// The JSON a contract verdict is written as.
pub(super) fn verdict_schema() -> &'static str {
    r#"```json
{"verdict":"contract-rulings","rulings":[{"contract":"rejects-x","ruling":"accept","reasoning":"...","citations":[{"file":"tests/x_contract.rs","line":1,"excerpt":"...","claim":"..."}]},{"contract":"rejects-y","ruling":"reject","reasoning":"...","citations":[{"file":"tests/y_contract.rs","line":1,"excerpt":"...","claim":"..."}]}],"plan_patch":{"field":"contracts","patches":[{"op":"replace","index":0,"value":"id: rejects-x\nfile: tests/x_contract.rs\ntest: rejects_x\nscenario: x arrives\nrejects: accepting x"}],"reason":"why the spec changes"}}
{"verdict":"needs-more-evidence","questions":["..."]}
```

Write one of these JSON objects. `rulings` has one entry for every disputed
contract: `contract` is its id and `ruling` is `accept` or `reject`. Each entry
needs `reasoning` and at least one citation. Omit `plan_patch` unless an
accepted contract's spec needs changing or deleting. If included, `field` must
be `contracts`, and each entry of `patches` has `op` (`replace` or `delete`),
`index` (the zero-based index in the stage's contracts array as it stands now,
before any patch applies) and, for a replace, `value`: a JSON string holding
the complete replacement contract in YAML (id, file, test, scenario, and
rejects, plus `runner` and `cwd` whenever the current spec sets them).
`cwd` is the optional directory the contract runs from, relative to the
worktree root, and always decides where it runs. Without one, a replacement
that keeps `file` keeps the recorded cwd, and one that changes `file` lets loom
detect the directory again. Patches apply only to accepted contracts.
needs-more-evidence needs at least one question.

"#
}
