//! E2E tests for loom hooks infrastructure
//!
//! These tests verify:
//! - Hook installation via loom init
//! - Commit message filtering (blocks Claude co-authorship attribution)
//! - Hook configuration in settings.local.json

use anyhow::{Context, Result};
use loom::fs::permissions::constants::HOOK_POST_TOOL_USE;
use serial_test::serial;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;
use tempfile::TempDir;

/// Create a temporary git repository for testing
fn create_test_git_repo() -> Result<TempDir> {
    let temp = TempDir::new().context("Failed to create temp directory")?;

    Command::new("git")
        .args(["init"])
        .current_dir(temp.path())
        .output()
        .context("Failed to run git init")?;

    Command::new("git")
        .args(["config", "user.email", "test@test.com"])
        .current_dir(temp.path())
        .output()
        .context("Failed to set git user.email")?;

    Command::new("git")
        .args(["config", "user.name", "Test User"])
        .current_dir(temp.path())
        .output()
        .context("Failed to set git user.name")?;

    fs::write(temp.path().join("README.md"), "# Test Repository\n")
        .context("Failed to write README.md")?;

    Command::new("git")
        .args(["add", "."])
        .current_dir(temp.path())
        .output()
        .context("Failed to git add")?;

    Command::new("git")
        .args(["commit", "-m", "Initial commit"])
        .current_dir(temp.path())
        .output()
        .context("Failed to git commit")?;

    Ok(temp)
}

/// Install the post-tool-use hook script to a temp directory for testing
fn install_test_hook(hooks_dir: &Path) -> Result<std::path::PathBuf> {
    fs::create_dir_all(hooks_dir).context("Failed to create hooks directory")?;

    let hook_path = hooks_dir.join("post-tool-use.sh");
    fs::write(&hook_path, HOOK_POST_TOOL_USE).context("Failed to write hook script")?;

    // Make executable (chmod +x)
    let mut perms = fs::metadata(&hook_path)
        .context("Failed to get metadata")?
        .permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&hook_path, perms).context("Failed to set permissions")?;

    Ok(hook_path)
}

/// Run the hook script with specified environment variables
///
/// Returns (exit_code, stdout, stderr)
fn run_hook(hook_path: &Path, tool_name: &str, tool_input: &str) -> Result<(i32, String, String)> {
    let output = Command::new("bash")
        .arg(hook_path)
        .env("TOOL_NAME", tool_name)
        .env("TOOL_INPUT", tool_input)
        // Set loom env vars to avoid the "silently exit if not in loom context" behavior
        // for the heartbeat update part (but we mainly care about the attribution check)
        .env("LOOM_STAGE_ID", "")
        .env("LOOM_SESSION_ID", "")
        .env("LOOM_WORK_DIR", "")
        .output()
        .context("Failed to run hook script")?;

    let exit_code = output.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    Ok((exit_code, stdout, stderr))
}

// ============================================================================
// Hook Installation Tests
// ============================================================================

/// Test that hook installation creates executable files
#[test]
#[serial]
fn test_hook_installation_creates_executable_files() {
    let temp_dir = TempDir::new().unwrap();
    let hooks_dir = temp_dir.path().join("hooks");

    let hook_path = install_test_hook(&hooks_dir).expect("Should install hook");

    // Verify hook exists
    assert!(hook_path.exists(), "Hook file should exist");

    // Verify hook is executable
    let metadata = fs::metadata(&hook_path).expect("Should get metadata");
    let mode = metadata.permissions().mode();
    assert!(mode & 0o111 != 0, "Hook should be executable");
}

/// Test that hook script contains the attribution check logic
#[test]
fn test_hook_contains_attribution_check() {
    // Verify the embedded hook has the attribution check
    assert!(
        HOOK_POST_TOOL_USE.contains("co-authored-by.*claude"),
        "Hook should contain co-authored-by check"
    );
    assert!(
        HOOK_POST_TOOL_USE.contains("claude.*(noreply|anthropic)"),
        "Hook should contain claude attribution check"
    );
    assert!(
        HOOK_POST_TOOL_USE.contains("BLOCKED"),
        "Hook should have BLOCKED message"
    );
}

// ============================================================================
// Commit Message Filtering Tests - Core Functionality
// ============================================================================

/// Test that hook blocks commits with "Co-Authored-By: Claude" attribution
#[test]
#[serial]
fn test_hook_blocks_claude_coauthor_simple() {
    let temp_dir = TempDir::new().unwrap();
    let hooks_dir = temp_dir.path().join("hooks");
    let hook_path = install_test_hook(&hooks_dir).expect("Should install hook");

    let commit_cmd = r#"git commit -m "$(cat <<'EOF'
Fix bug in parser

Co-Authored-By: Claude <noreply@anthropic.com>
EOF
)""#;

    let (exit_code, stdout, _stderr) = run_hook(&hook_path, "Bash", commit_cmd).unwrap();

    assert_eq!(exit_code, 1, "Hook should exit with code 1 (blocked)");
    assert!(
        stdout.contains("BLOCKED"),
        "Output should contain BLOCKED message"
    );
    assert!(
        stdout.contains("Claude attribution detected"),
        "Output should explain what was blocked"
    );
}

/// Test that hook blocks "Co-Authored-By: Claude Opus" variations
#[test]
#[serial]
fn test_hook_blocks_claude_opus_coauthor() {
    let temp_dir = TempDir::new().unwrap();
    let hooks_dir = temp_dir.path().join("hooks");
    let hook_path = install_test_hook(&hooks_dir).expect("Should install hook");

    let commit_cmd = r#"git commit -m "Add feature

Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>
""#;

    let (exit_code, stdout, _stderr) = run_hook(&hook_path, "Bash", commit_cmd).unwrap();

    assert_eq!(exit_code, 1, "Hook should exit with code 1 (blocked)");
    assert!(
        stdout.contains("BLOCKED"),
        "Output should contain BLOCKED message"
    );
}

/// Test that hook blocks commits mentioning "claude" with "anthropic"
#[test]
#[serial]
fn test_hook_blocks_claude_anthropic_mention() {
    let temp_dir = TempDir::new().unwrap();
    let hooks_dir = temp_dir.path().join("hooks");
    let hook_path = install_test_hook(&hooks_dir).expect("Should install hook");

    let commit_cmd = r#"git commit -m "Feature assisted by claude noreply@anthropic.com""#;

    let (exit_code, stdout, _stderr) = run_hook(&hook_path, "Bash", commit_cmd).unwrap();

    assert_eq!(exit_code, 1, "Hook should exit with code 1 (blocked)");
    assert!(stdout.contains("BLOCKED"), "Output should contain BLOCKED");
}

/// Test that hook blocks case-insensitive variations
#[test]
#[serial]
fn test_hook_blocks_case_insensitive() {
    let temp_dir = TempDir::new().unwrap();
    let hooks_dir = temp_dir.path().join("hooks");
    let hook_path = install_test_hook(&hooks_dir).expect("Should install hook");

    let commit_cmd = r#"git commit -m "Fix

CO-AUTHORED-BY: CLAUDE <noreply@anthropic.com>
""#;

    let (exit_code, stdout, _stderr) = run_hook(&hook_path, "Bash", commit_cmd).unwrap();

    assert_eq!(exit_code, 1, "Hook should exit with code 1 (blocked)");
    assert!(stdout.contains("BLOCKED"), "Output should contain BLOCKED");
}

// ============================================================================
// Commit Message Filtering Tests - Allowed Commits
// ============================================================================

/// Test that hook allows commits without Claude attribution
#[test]
#[serial]
fn test_hook_allows_clean_commit() {
    let temp_dir = TempDir::new().unwrap();
    let hooks_dir = temp_dir.path().join("hooks");
    let hook_path = install_test_hook(&hooks_dir).expect("Should install hook");

    let commit_cmd = r#"git commit -m "Fix bug in parser""#;

    let (exit_code, stdout, _stderr) = run_hook(&hook_path, "Bash", commit_cmd).unwrap();

    assert_eq!(exit_code, 0, "Hook should exit with code 0 (allowed)");
    assert!(
        !stdout.contains("BLOCKED"),
        "Output should not contain BLOCKED message"
    );
}

/// Test that hook allows commits with other co-authors (not Claude)
#[test]
#[serial]
fn test_hook_allows_human_coauthor() {
    let temp_dir = TempDir::new().unwrap();
    let hooks_dir = temp_dir.path().join("hooks");
    let hook_path = install_test_hook(&hooks_dir).expect("Should install hook");

    let commit_cmd = r#"git commit -m "Feature implementation

Co-Authored-By: Jane Doe <jane@example.com>
Co-Authored-By: John Smith <john@example.com>
""#;

    let (exit_code, stdout, _stderr) = run_hook(&hook_path, "Bash", commit_cmd).unwrap();

    assert_eq!(exit_code, 0, "Hook should exit with code 0 (allowed)");
    assert!(
        !stdout.contains("BLOCKED"),
        "Output should not contain BLOCKED message"
    );
}

/// Test that hook allows commits mentioning "claude" in commit message (not co-author)
#[test]
#[serial]
fn test_hook_allows_claude_in_message_body() {
    let temp_dir = TempDir::new().unwrap();
    let hooks_dir = temp_dir.path().join("hooks");
    let hook_path = install_test_hook(&hooks_dir).expect("Should install hook");

    // Mentioning "claude" in the commit body without the co-author pattern
    // should be allowed (the regex is: co-authored-by.*claude|claude.*(noreply|anthropic))
    let commit_cmd = r#"git commit -m "Fix the Claude class naming convention""#;

    let (exit_code, stdout, _stderr) = run_hook(&hook_path, "Bash", commit_cmd).unwrap();

    assert_eq!(exit_code, 0, "Hook should exit with code 0 (allowed)");
    assert!(
        !stdout.contains("BLOCKED"),
        "Output should not contain BLOCKED message"
    );
}

/// Test that hook ignores non-Bash tools
#[test]
#[serial]
fn test_hook_ignores_non_bash_tools() {
    let temp_dir = TempDir::new().unwrap();
    let hooks_dir = temp_dir.path().join("hooks");
    let hook_path = install_test_hook(&hooks_dir).expect("Should install hook");

    // Even with Claude attribution in input, non-Bash tools should pass
    let commit_cmd = r#"git commit -m "Fix

Co-Authored-By: Claude <noreply@anthropic.com>
""#;

    let (exit_code, stdout, _stderr) = run_hook(&hook_path, "Write", commit_cmd).unwrap();

    assert_eq!(
        exit_code, 0,
        "Hook should exit with code 0 for non-Bash tools"
    );
    assert!(
        !stdout.contains("BLOCKED"),
        "Output should not contain BLOCKED message"
    );
}

/// Test that hook ignores non-commit git commands
#[test]
#[serial]
fn test_hook_ignores_non_commit_git_commands() {
    let temp_dir = TempDir::new().unwrap();
    let hooks_dir = temp_dir.path().join("hooks");
    let hook_path = install_test_hook(&hooks_dir).expect("Should install hook");

    // git status, git log, etc. should pass even with Claude mention
    let git_cmd = r#"git log --author="Claude""#;

    let (exit_code, stdout, _stderr) = run_hook(&hook_path, "Bash", git_cmd).unwrap();

    assert_eq!(
        exit_code, 0,
        "Hook should exit with code 0 for non-commit commands"
    );
    assert!(
        !stdout.contains("BLOCKED"),
        "Output should not contain BLOCKED message"
    );
}

// ============================================================================
// Full Integration Test - Git Repo + Hook + Commit Verification
// ============================================================================

/// Comprehensive integration test that:
/// 1. Creates a git repository
/// 2. Installs the hook
/// 3. Verifies the hook blocks Claude-attributed commits
/// 4. Verifies clean commits are allowed
/// 5. Ensures no Claude attribution appears in the git history
#[test]
#[serial]
fn test_full_hook_integration() {
    let temp_repo = create_test_git_repo().expect("Should create test repo");
    let temp_hooks = TempDir::new().expect("Should create temp hooks dir");
    let hooks_dir = temp_hooks.path();
    let hook_path = install_test_hook(hooks_dir).expect("Should install hook");

    // Test 1: Verify clean commit would be allowed
    let clean_commit = r#"git commit -m "Add new feature""#;
    let (exit_code, _, _) = run_hook(&hook_path, "Bash", clean_commit).unwrap();
    assert_eq!(exit_code, 0, "Clean commit should be allowed");

    // Test 2: Verify Claude attribution would be blocked
    let claude_commit = r#"git commit -m "Add feature

Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>
""#;
    let (exit_code, stdout, _) = run_hook(&hook_path, "Bash", claude_commit).unwrap();
    assert_eq!(exit_code, 1, "Claude commit should be blocked");
    assert!(stdout.contains("BLOCKED"), "Should show BLOCKED message");
    assert!(
        stdout.contains("CLAUDE.md rule 7"),
        "Should reference CLAUDE.md rule"
    );

    // Test 3: Actually create some commits in the repo and verify history
    // First, make a change
    fs::write(temp_repo.path().join("feature.txt"), "New feature code\n")
        .expect("Should write file");

    Command::new("git")
        .args(["add", "feature.txt"])
        .current_dir(temp_repo.path())
        .output()
        .expect("Should git add");

    // Commit without Claude attribution (simulating what happens when hook blocks
    // and user retries without the attribution)
    Command::new("git")
        .args(["commit", "-m", "Add feature without AI attribution"])
        .current_dir(temp_repo.path())
        .output()
        .expect("Should git commit");

    // Test 4: Verify git history contains NO Claude attribution
    let log_output = Command::new("git")
        .args(["log", "--format=full"])
        .current_dir(temp_repo.path())
        .output()
        .expect("Should get git log");

    let log_str = String::from_utf8_lossy(&log_output.stdout);

    assert!(
        !log_str.to_lowercase().contains("co-authored-by: claude"),
        "Git history should not contain Claude co-author"
    );
    assert!(
        !log_str.contains("noreply@anthropic.com"),
        "Git history should not contain anthropic email"
    );
}

// ============================================================================
// Edge Case Tests
// ============================================================================

/// Test hook handles empty TOOL_INPUT gracefully
#[test]
#[serial]
fn test_hook_handles_empty_input() {
    let temp_dir = TempDir::new().unwrap();
    let hooks_dir = temp_dir.path().join("hooks");
    let hook_path = install_test_hook(&hooks_dir).expect("Should install hook");

    let (exit_code, _, _) = run_hook(&hook_path, "Bash", "").unwrap();

    assert_eq!(
        exit_code, 0,
        "Hook should exit successfully with empty input"
    );
}

/// Test hook handles missing environment variables gracefully
#[test]
#[serial]
fn test_hook_handles_missing_env_vars() {
    let temp_dir = TempDir::new().unwrap();
    let hooks_dir = temp_dir.path().join("hooks");
    let hook_path = install_test_hook(&hooks_dir).expect("Should install hook");

    // Run without setting any env vars
    let output = Command::new("bash")
        .arg(&hook_path)
        .output()
        .expect("Should run hook");

    let exit_code = output.status.code().unwrap_or(-1);
    assert_eq!(
        exit_code, 0,
        "Hook should exit successfully with missing env vars"
    );
}

/// Test hook correctly blocks git commit with heredoc syntax variants
#[test]
#[serial]
fn test_hook_blocks_various_heredoc_formats() {
    let temp_dir = TempDir::new().unwrap();
    let hooks_dir = temp_dir.path().join("hooks");
    let hook_path = install_test_hook(&hooks_dir).expect("Should install hook");

    // Different heredoc formats that Claude might use
    let heredoc_variants = [
        // Standard heredoc with quotes
        r#"git commit -m "$(cat <<'EOF'
Fix bug

Co-Authored-By: Claude <noreply@anthropic.com>
EOF
)""#,
        // Heredoc without quotes
        r#"git commit -m "$(cat <<EOF
Add feature

Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>
EOF
)""#,
        // Inline format
        r#"git commit -m "Feature

Co-Authored-By: Claude <noreply@anthropic.com>""#,
    ];

    for (i, cmd) in heredoc_variants.iter().enumerate() {
        let (exit_code, stdout, _) = run_hook(&hook_path, "Bash", cmd).unwrap();
        assert_eq!(exit_code, 1, "Variant {} should be blocked: {}", i, cmd);
        assert!(
            stdout.contains("BLOCKED"),
            "Variant {} should show BLOCKED",
            i
        );
    }
}

/// Test that the hook message provides helpful guidance
#[test]
#[serial]
fn test_hook_provides_helpful_message() {
    let temp_dir = TempDir::new().unwrap();
    let hooks_dir = temp_dir.path().join("hooks");
    let hook_path = install_test_hook(&hooks_dir).expect("Should install hook");

    let commit_cmd = r#"git commit -m "Fix

Co-Authored-By: Claude <noreply@anthropic.com>
""#;

    let (exit_code, stdout, _) = run_hook(&hook_path, "Bash", commit_cmd).unwrap();

    assert_eq!(exit_code, 1, "Should be blocked");

    // Verify helpful guidance is provided
    assert!(
        stdout.contains("Remove the Co-Authored-By line"),
        "Should tell user to remove the line"
    );
    assert!(
        stdout.contains("AI attribution is FORBIDDEN"),
        "Should explain why it's forbidden"
    );
    assert!(
        stdout.contains("CLAUDE.md rule 7"),
        "Should reference the rule"
    );
}

// ============================================================================
// Loom-Specific Integration Tests
// ============================================================================

/// Test that loom's ensure_loom_permissions includes hooks configuration
#[test]
#[serial]
fn test_loom_permissions_include_hooks() {
    use loom::fs::permissions::ensure_loom_permissions;

    let temp_dir = TempDir::new().unwrap();
    let repo_root = temp_dir.path();

    ensure_loom_permissions(repo_root).expect("Should configure permissions");

    let settings_path = repo_root.join(".claude/settings.local.json");
    assert!(settings_path.exists(), "Settings file should exist");

    let content = fs::read_to_string(&settings_path).expect("Should read settings");
    let settings: serde_json::Value = serde_json::from_str(&content).expect("Should parse JSON");

    // Verify hooks are configured
    assert!(
        settings.get("hooks").is_some(),
        "hooks section should exist"
    );

    let hooks = settings.get("hooks").unwrap();
    assert!(
        hooks.get("Stop").is_some(),
        "Stop hooks should be configured"
    );
}

/// Test that install_loom_hooks creates hook files in the correct location
#[test]
#[serial]
fn test_install_loom_hooks_creates_files() {
    use loom::fs::permissions::install_loom_hooks;

    // This test creates files in ~/.claude/hooks/loom/ which is the real location
    let result = install_loom_hooks();
    assert!(result.is_ok(), "Hook installation should succeed");

    // Verify hooks exist
    let home_dir = dirs::home_dir().expect("Should have home dir");
    let hooks_dir = home_dir.join(".claude/hooks/loom");

    if hooks_dir.exists() {
        // Verify key hooks are installed
        assert!(
            hooks_dir.join("post-tool-use.sh").exists(),
            "post-tool-use.sh should exist"
        );
        assert!(
            hooks_dir.join("commit-guard.sh").exists(),
            "commit-guard.sh should exist"
        );

        // Verify post-tool-use.sh contains the attribution check
        let content =
            fs::read_to_string(hooks_dir.join("post-tool-use.sh")).expect("Should read hook");
        assert!(
            content.contains("co-authored-by.*claude"),
            "Hook should contain attribution check"
        );
    }
}
