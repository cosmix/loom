# ISSUES

---

1. signal files should not contain knowledge content! That increases the context significantly. They should ONLY contain memory records verbatim AND GUIDE THE AGENT to use knowledge using `loom knowledge` commands.
2. ~~the loom orchestrator should update plan prefix to DONE- when a plan is SUCCESSFULLY completed.~~ **FIXED** - Bug was in `daemon/server/orchestrator.rs:110` where `WorkDir::new(work_dir)` was called with the `.work/` path, but `WorkDir::new()` expects repo root and appends `.work` internally. This caused it to look for `.work/.work` which doesn't exist. Fixed by using `repo_root` instead.
3. add sandbox mode to loom orchestrator for better control/security and determinism during plan execution. This will prevent loom from trying to access resources directly and use the loom cli.
4. We're getting
   ● Ran 2 stop hooks  
    ⎿  Stop hook prevented continuation

   I want to be sure that this is expected and what the result of those hooks is.
5. The acceptance criteria have incorrect paths. For example, when working on loom itself, stage files acceptance criteria,  specify loom/src/... but since working_dir is loom, those paths become loom/loom/src/.... Let me verify the content is correct and record this issue.
6. ~~loom hooks install should OVERWRITE existing hooks, not ignore them if they already exist or append to each file. The user intent is clear and there may be updates to the hooks.~~ **RESOLVED** - Modified `configure_loom_hooks()` in `fs/permissions/hooks.rs` to remove all existing loom hooks (scripts in `~/.claude/hooks/loom/`) before adding fresh ones. This ensures users always get the latest hook configuration when running `loom hooks install`.
