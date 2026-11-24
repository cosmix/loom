//! Reachability of keyword-index keys through the skill-trigger hook's own
//! tokenizer. A key the tokenizer can never produce, whatever the prompt, is a
//! skill keyword that silently never fires (`c++`, `.net`, `n+1`, long phrases).

use super::*;
use crate::helpers::run_hook_process;
use std::fs;
use std::path::Path;
use std::process::Command;
use tempfile::TempDir;

/// Loads the installed hook as a module and tokenizes with its own `_tokens`.
/// `reach` prints the index keys that are not produced when the key itself is
/// the prompt; `tokens` reads a JSON list of prompts from stdin and prints the
/// sorted token list of each.
const TOKENIZER_DRIVER: &str = r#"
import importlib.machinery, importlib.util, json, os, sys
hook, index_path, mode = sys.argv[1:4]
sys.path.insert(0, os.path.dirname(hook))
sys.argv = [hook]
loader = importlib.machinery.SourceFileLoader("skill_trigger", hook)
spec = importlib.util.spec_from_loader("skill_trigger", loader)
module = importlib.util.module_from_spec(spec)
loader.exec_module(module)
with open(index_path) as source:
    index = json.load(source)
if mode == "reach":
    print(json.dumps([k for k in index if k not in module._tokens(k, index)]))
else:
    prompts = json.load(sys.stdin)
    print(json.dumps([sorted(module._tokens(p, index)) for p in prompts]))
"#;

fn run_driver(hook: &Path, index: &Path, mode: &str, stdin: &str) -> serde_json::Value {
    let mut cmd = Command::new("python3");
    cmd.args(["-B", "-c", TOKENIZER_DRIVER])
        .arg(hook)
        .arg(index)
        .arg(mode);
    let output = run_hook_process(&mut cmd, stdin.as_bytes());
    assert!(
        output.status.success(),
        "tokenizer driver failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("driver prints JSON")
}

/// Build the real indexer's output over every skill under the repository's
/// `skills/` directory and return the index path.
fn build_real_index(root: &Path) -> std::path::PathBuf {
    let skills = Path::new(env!("CARGO_MANIFEST_DIR")).join("../skills");
    for entry in fs::read_dir(&skills).expect("read repository skills dir") {
        let source = entry.expect("skill dir entry").path().join("SKILL.md");
        if !source.is_file() {
            continue;
        }
        let target = root
            .join("loom-skill-catalog")
            .join(source.parent().unwrap().file_name().unwrap())
            .join("SKILL.md");
        fs::create_dir_all(target.parent().unwrap()).expect("create catalog skill dir");
        fs::copy(&source, &target).expect("copy SKILL.md into catalog");
    }
    loom::commands::skill_index::execute_in_agent_dir(root, false).expect("build real index");
    root.join("hooks/loom/skill-keywords.json")
}

#[test]
fn every_real_index_key_is_reachable_by_the_tokenizer() {
    if skip_unless_python3("hooks_skill_trigger::reach::every_real_index_key_is_reachable") {
        return;
    }
    let root = TempDir::new().expect("create agent dir");
    let index_path = build_real_index(root.path());
    let index: serde_json::Map<String, serde_json::Value> =
        serde_json::from_str(&fs::read_to_string(&index_path).expect("read index"))
            .expect("parse index");
    assert!(
        index.len() > 100,
        "the real index looks truncated: {} keys",
        index.len()
    );
    for known in ["c++", "c#", ".net", "n+1", "node:test", "std::span"] {
        assert!(
            index.contains_key(known),
            "skills no longer index {known:?}"
        );
    }
    let (_hook_dir, hook) = install_hook();
    let missing = run_driver(&hook, &index_path, "reach", "");
    assert_eq!(
        missing,
        serde_json::json!([]),
        "keywords no prompt can ever match: rewrite them in the skill frontmatter or \
         teach loom-hooks/_skill_tokens.py to produce them"
    );
}

fn synthetic_index() -> TempDir {
    let dir = TempDir::new().expect("create index dir");
    let keys = [
        "c++",
        "c#",
        ".net",
        "n+1",
        "problem+json",
        "node:test",
        "std::span",
        "software bill of materials",
        "passes locally fails in ci",
        "sentence",
        "foo",
        "bar",
        "env",
        "ci/cd",
    ];
    let index: serde_json::Map<String, serde_json::Value> = keys
        .iter()
        .map(|key| (key.to_string(), serde_json::json!(["loom-x"])))
        .collect();
    fs::write(
        dir.path().join("index.json"),
        serde_json::Value::Object(index).to_string(),
    )
    .expect("write index");
    dir
}

fn tokens_of(prompts: &[&str]) -> Vec<std::collections::BTreeSet<String>> {
    let (_hook_dir, hook) = install_hook();
    let index_dir = synthetic_index();
    let out = run_driver(
        &hook,
        &index_dir.path().join("index.json"),
        "tokens",
        &serde_json::json!(prompts).to_string(),
    );
    serde_json::from_value(out).expect("token lists")
}

#[test]
fn natural_prompts_reach_symbol_and_long_phrase_keywords() {
    if skip_unless_python3("hooks_skill_trigger::reach::natural_prompts_reach_keywords") {
        return;
    }
    let cases = [
        ("fix this C++ crash", "c++"),
        ("is this C# or F# code", "c#"),
        ("my .NET app is slow", ".net"),
        ("an N+1 query in the orm", "n+1"),
        ("return application/problem+json errors", "problem+json"),
        ("use node:test for this", "node:test"),
        ("convert it to std::span please", "std::span"),
        (
            "do we need a software bill of materials?",
            "software bill of materials",
        ),
        (
            "it passes locally fails in CI again",
            "passes locally fails in ci",
        ),
        ("set up the CI/CD pipeline", "ci/cd"),
        ("source the .env file", "env"),
    ];
    let prompts: Vec<&str> = cases.iter().map(|(prompt, _)| *prompt).collect();
    for ((prompt, key), tokens) in cases.iter().zip(tokens_of(&prompts)) {
        assert!(
            tokens.contains(*key),
            "{prompt:?} did not produce {key:?}: {tokens:?}"
        );
    }
}

#[test]
fn ordinary_punctuation_does_not_glue_words_together() {
    if skip_unless_python3("hooks_skill_trigger::reach::ordinary_punctuation_stays_a_separator") {
        return;
    }
    let results = tokens_of(&[
        "this is the end of the sentence.",
        "foo: bar",
        "foo, bar; (foo) [bar]",
        "see https://example.com/docs/page.html now",
        "a plain c, plus a .env and a net",
    ]);
    assert!(results[0].contains("sentence") && !results[0].contains("sentence."));
    assert!(results[1].contains("foo") && results[1].contains("bar"));
    assert!(
        !results[1].iter().any(|t| t.contains(':')),
        "{:?}",
        results[1]
    );
    assert!(results[2].contains("foo") && results[2].contains("bar"));
    assert!(results[3].contains("example.com/docs/page.html"));
    assert!(
        !results[3].iter().any(|t| t.starts_with("https:")),
        "{:?}",
        results[3]
    );
    // Words that merely resemble a symbol keyword must not produce it.
    for key in ["c++", "c#", ".net", "n+1"] {
        assert!(!results[4].contains(key), "{key:?} in {:?}", results[4]);
    }
}
