//! Tests for hooks configuration

use crate::fs::permissions::hooks::loom_hooks_config;

#[test]
fn test_hooks_config_structure() {
    let hooks = loom_hooks_config();
    let hooks_obj = hooks.as_object().unwrap();

    // Check PreToolUse hooks (AskUserQuestion for stage status, Bash for prefer-modern-tools, Bash for commit-filter)
    let pre_tool = hooks_obj.get("PreToolUse").unwrap().as_array().unwrap();
    assert_eq!(pre_tool.len(), 3);
    // First hook: AskUserQuestion matcher with ask-user-pre.sh
    assert_eq!(pre_tool[0]["matcher"], "AskUserQuestion");
    assert!(pre_tool[0]["hooks"][0]["command"]
        .as_str()
        .unwrap()
        .contains("ask-user-pre.sh"));
    // Second hook: Bash matcher with prefer-modern-tools.sh
    assert_eq!(pre_tool[1]["matcher"], "Bash");
    assert!(pre_tool[1]["hooks"][0]["command"]
        .as_str()
        .unwrap()
        .contains("prefer-modern-tools.sh"));
    // Third hook: Bash matcher with commit-filter.sh
    assert_eq!(pre_tool[2]["matcher"], "Bash");
    assert!(pre_tool[2]["hooks"][0]["command"]
        .as_str()
        .unwrap()
        .contains("commit-filter.sh"));

    // Check PostToolUse hooks (only AskUserQuestion for resume in global config)
    // Session-specific post-tool-use.sh (Bash) is merged at worktree creation
    let post_tool = hooks_obj.get("PostToolUse").unwrap().as_array().unwrap();
    assert_eq!(post_tool.len(), 1);
    // Only hook: AskUserQuestion matcher with ask-user-post.sh (stage resume)
    assert_eq!(post_tool[0]["matcher"], "AskUserQuestion");
    assert!(post_tool[0]["hooks"][0]["command"]
        .as_str()
        .unwrap()
        .contains("ask-user-post.sh"));

    // Check Stop hook
    let stop = hooks_obj.get("Stop").unwrap().as_array().unwrap();
    assert_eq!(stop.len(), 1);
    assert!(stop[0]["hooks"][0]["command"]
        .as_str()
        .unwrap()
        .contains("commit-guard.sh"));

    // Check UserPromptSubmit hook (skill suggestions)
    let user_prompt = hooks_obj
        .get("UserPromptSubmit")
        .unwrap()
        .as_array()
        .unwrap();
    assert_eq!(user_prompt.len(), 1);
    assert_eq!(user_prompt[0]["matcher"], "*");
    assert!(user_prompt[0]["hooks"][0]["command"]
        .as_str()
        .unwrap()
        .contains("skill-trigger.sh"));
}
