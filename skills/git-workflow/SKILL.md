---
name: git-workflow
description: Manages Git operations including branching strategies, commit conventions, merge workflows, conflict resolution, and worktree management. Trigger keywords: git, branch, commit, merge, rebase, pull request, PR, cherry-pick, stash, reset, revert, checkout, switch, worktree, conflict, GitFlow, trunk-based, feature branch, release branch, version control, squash, amend, history, tag, remote, push, pull, fetch, clone.
allowed-tools: Read, Grep, Glob, Bash
---

# Git Workflow

## Overview

This skill provides guidance on Git best practices, branching strategies, commit conventions, and collaborative workflows. It helps maintain a clean and navigable version control history.

## Instructions

### 1. Branch Management

- Follow consistent naming conventions (feature/, fix/, hotfix/, release/)
- Create branches from appropriate base (main, develop, release branch)
- Keep branches focused and short-lived
- Delete merged branches promptly
- Use descriptive names with issue references

### 2. Commit Practices

- Write meaningful commit messages (conventional commits: feat, fix, docs, refactor, test, chore)
- Make atomic commits (one logical change per commit)
- Use conventional commit format with optional scope and breaking changes
- Include issue references and context in commit body
- Amend commits carefully (only for unpushed commits)

### 3. Merge Strategies

- Choose appropriate strategy: merge commit, squash, or rebase
- Review changes before merging (use pull requests)
- Resolve conflicts carefully with context awareness
- Maintain clean history (interactive rebase before merge)
- Never force push to protected branches

### 4. Worktree Management

- Use worktrees for parallel work without branch switching
- Create worktrees for isolated feature development or hotfixes
- List active worktrees and clean up when done
- Share .git directory while maintaining separate working directories
- Ideal for testing, reviewing PRs, or working on multiple features simultaneously

### 5. Conflict Resolution

- Understand conflict markers (<<<<<<, ======, >>>>>>)
- Check full context before resolving (git diff, git log)
- Use merge tools for complex conflicts (git mergetool)
- Test after resolution (run tests, verify functionality)
- Document non-obvious resolutions in commit message

### 6. Collaboration

- Keep branches up to date with base branch
- Use pull requests for code review
- Squash commits when appropriate (clean up WIP commits)
- Protect important branches (main, master, production)
- Communicate breaking changes clearly

## Best Practices

1. **Atomic Commits**: Each commit should represent one logical change
2. **Meaningful Messages**: Describe what and why, not how (conventional commit format)
3. **Branch Often**: Use feature branches for all changes
4. **Pull Before Push**: Stay synchronized with remote (fetch + rebase or merge)
5. **Review Before Merge**: All changes should be reviewed (pull requests)
6. **Protect Main**: Never force push to main/master/production
7. **Clean History**: Squash WIP commits before merging (interactive rebase)
8. **Worktrees for Parallel Work**: Use worktrees instead of stashing when working on multiple features
9. **Test After Conflicts**: Always run tests after resolving merge conflicts
10. **Document Breaking Changes**: Use BREAKING CHANGE footer in commit messages

## Examples

### Example 1: Conventional Commit Messages

```bash
# Format: <type>(<scope>): <description>
# Types: feat, fix, docs, style, refactor, test, chore

# Feature addition
git commit -m "feat(auth): add OAuth2 login with Google provider"

# Bug fix with issue reference
git commit -m "fix(cart): resolve race condition in quantity update

When rapidly clicking add/remove, the cart count could become negative
due to unsynchronized state updates.

Fixes #234"

# Breaking change
git commit -m "feat(api)!: change user endpoint response format

BREAKING CHANGE: The /users endpoint now returns paginated results
instead of an array. Clients must update to handle the new format.

Migration guide: https://docs.example.com/migration/v2"

# Documentation
git commit -m "docs(readme): add installation instructions for Windows"

# Refactoring
git commit -m "refactor(db): extract query builder into separate module"
```

### Example 2: Branch Naming Conventions

```bash
# Feature branches
git checkout -b feature/user-authentication
git checkout -b feature/JIRA-123-shopping-cart

# Bug fix branches
git checkout -b fix/login-redirect-loop
git checkout -b fix/JIRA-456-null-pointer

# Hotfix branches (production issues)
git checkout -b hotfix/security-patch-xss

# Release branches
git checkout -b release/v2.1.0

# Experiment branches
git checkout -b experiment/new-caching-strategy
```

### Example 3: Git Workflow Commands

```bash
# Start new feature
git checkout main
git pull origin main
git checkout -b feature/new-feature

# Regular development cycle
git add -A
git commit -m "feat: implement feature part 1"
git push -u origin feature/new-feature

# Keep branch updated with main
git fetch origin
git rebase origin/main
# Or merge if preferred
git merge origin/main

# Interactive rebase to clean up commits before PR
git rebase -i origin/main
# In editor: squash, reword, or reorder commits

# After PR approval, merge and cleanup
git checkout main
git pull origin main
git branch -d feature/new-feature
git push origin --delete feature/new-feature

# Handling merge conflicts
git merge feature-branch
# If conflicts occur:
git status  # See conflicted files
# Edit files to resolve conflicts
git add <resolved-files>
git merge --continue

# Undo last commit (keep changes)
git reset --soft HEAD~1

# Undo last commit (discard changes)
git reset --hard HEAD~1

# Cherry-pick specific commit
git cherry-pick abc123

# Create annotated tag for release
git tag -a v1.0.0 -m "Release version 1.0.0"
git push origin v1.0.0
```

### Example 4: Worktree Workflows

```bash
# List existing worktrees
git worktree list

# Create worktree for new feature (creates branch and worktree)
git worktree add ../feature-auth feature/user-auth

# Create worktree from existing branch
git worktree add ../hotfix-123 hotfix/critical-bug

# Create worktree with detached HEAD for testing
git worktree add --detach ../testing-v1.2.0 v1.2.0

# Work in the worktree
cd ../feature-auth
git status
# Make changes, commit normally
git add .
git commit -m "feat(auth): implement OAuth2 flow"

# Remove worktree when done (must be on different branch first)
cd /original/repo
git worktree remove ../feature-auth
# Or if worktree directory was manually deleted
git worktree prune

# Loom-specific: Stages execute in isolated worktrees
# Each stage gets .worktrees/<stage-id>/ with branch loom/<stage-id>
# This enables true parallel execution without file conflicts
ls .worktrees/
# knowledge-bootstrap/
# implement-auth/
# add-tests/
```

### Example 5: Conflict Resolution Workflow

```bash
# Attempt merge that results in conflicts
git merge feature/new-api
# Auto-merging src/api.rs
# CONFLICT (content): Merge conflict in src/api.rs
# Automatic merge failed; fix conflicts and then commit the result.

# Check which files have conflicts
git status
# both modified: src/api.rs
# both modified: src/config.rs

# View the conflict in context
git diff src/api.rs

# Conflict markers in file:
# <<<<<<< HEAD
# fn handle_request() {
#     // Current implementation
# }
# =======
# async fn handle_request() {
#     // New async implementation
# }
# >>>>>>> feature/new-api

# Option 1: Manually resolve in editor
# Edit src/api.rs, remove markers, keep desired code
vim src/api.rs

# Option 2: Use merge tool
git mergetool

# Option 3: Choose one side completely
git checkout --ours src/config.rs    # Keep current branch version
git checkout --theirs src/config.rs  # Take incoming branch version

# After resolving conflicts, stage resolved files
git add src/api.rs src/config.rs

# Verify resolution
cargo test  # Run tests to ensure nothing broke
git diff --staged

# Complete the merge
git commit -m "Merge feature/new-api

Resolved conflicts in api.rs by combining sync and async patterns.
Kept current config.rs authentication settings."

# If merge gets too complex, abort and try different approach
git merge --abort
git rebase feature/new-api  # Try rebase instead
```

### Example 6: Advanced Git Operations

```bash
# Stash work in progress
git stash push -m "WIP: half-done feature"
git stash list
git stash pop  # Apply and remove from stash
git stash apply stash@{1}  # Apply specific stash without removing

# Cherry-pick range of commits
git cherry-pick abc123..def456
git cherry-pick abc123 def456 ghi789  # Pick specific commits

# Revert commit (creates new commit that undoes changes)
git revert abc123
git revert --no-commit abc123..def456  # Revert range without committing

# Reset to earlier state
git reset --soft HEAD~3   # Keep changes staged
git reset --mixed HEAD~3  # Keep changes unstaged (default)
git reset --hard HEAD~3   # Discard changes completely

# Reflog: recover "lost" commits
git reflog
git checkout abc123  # Restore to commit that was reset away

# Clean untracked files
git clean -n  # Dry run (show what would be deleted)
git clean -fd  # Force delete untracked files and directories

# Bisect to find bug introduction
git bisect start
git bisect bad  # Current commit is bad
git bisect good v1.2.0  # This version was good
# Git checks out middle commit, test it
git bisect good  # or git bisect bad
# Repeat until git finds the problematic commit
git bisect reset  # Exit bisect mode
```

### Example 7: Git Aliases for Productivity

```bash
# Add to ~/.gitconfig
[alias]
    co = checkout
    br = branch
    ci = commit
    st = status
    lg = log --oneline --graph --decorate
    unstage = reset HEAD --
    last = log -1 HEAD
    amend = commit --amend --no-edit
    wip = commit -am "WIP"
    undo = reset --soft HEAD~1
    branches = branch -a
    tags = tag -l
    stashes = stash list

    # Show branches sorted by last commit date
    recent = for-each-ref --sort=-committerdate refs/heads/ --format='%(refname:short) %(committerdate:relative)'

    # Delete all merged branches
    cleanup = "!git branch --merged | grep -v '\\*\\|main\\|master' | xargs -n 1 git branch -d"

    # Worktree shortcuts
    wt = worktree
    wtls = worktree list
    wtadd = worktree add
    wtrm = worktree remove
```
