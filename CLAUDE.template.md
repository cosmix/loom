# Claude Code Rules

---

# 🛑 STOP. READ THIS FIRST. THIS IS NOT OPTIONAL.

## RULE ZERO: NO PLACEHOLDER CODE. EVER. NO EXCEPTIONS.

**THIS IS THE MOST IMPORTANT RULE. VIOLATING THIS RULE IS AN AUTOMATIC FAILURE.**

### YOU ARE ABSOLUTELY FORBIDDEN FROM WRITING:

- `// TODO` — **BANNED**
- `// FIXME` — **BANNED**
- `// implement later` — **BANNED**
- `// add logic here` — **BANNED**
- `pass` with no implementation — **BANNED**
- `return null` as a stub — **BANNED**
- `throw new Error("not implemented")` — **BANNED**
- Empty function bodies — **BANNED**
- Comments describing what code SHOULD do instead of ACTUAL CODE — **BANNED**
- Pseudocode instead of real code — **BANNED**
- Comments stating that 'in production code this would be implemented as X' — **BANNED**

### WHAT YOU MUST DO INSTEAD:

- **IMPLEMENT THE ACTUAL CODE.** Not tomorrow. Not later. NOW.
- If you don't know how to implement something: **STOP AND ASK.** Do NOT stub it.
- If it's too complex: **BREAK IT DOWN.** Do NOT leave placeholders.
- Every function you write MUST BE COMPLETE AND WORKING.

### SUBAGENT WARNING:

**YOU (the main agent) MUST COPY THIS ENTIRE "RULE ZERO" SECTION INTO EVERY SUBAGENT PROMPT.**

Subagents WILL create placeholder code unless you EXPLICITLY tell them not to. This is YOUR responsibility. If a subagent creates placeholder code, YOU failed to pass this rule.

---

## ⚠️ MANDATORY RULES

### 1. NATIVE TOOLS — NOT CLI

**THESE COMMANDS ARE BANNED. DO NOT USE THEM:**

`cat` `head` `tail` `less` `more` → **Use Read tool**
`grep` `rg` `ag` `ack` → **Use Grep tool**
`find` `ls` `fd` `tree` → **Use Glob tool**
`sed` `awk` `perl -pe` → **Use Edit tool**
`echo >` `cat <<EOF` `printf >` `tee` → **Use Write tool**
`curl` `wget` → **Use WebFetch tool**

**ONLY EXCEPTIONS:** `git`, `npm`, `docker`, `make`, `cargo`, `python`, `node` — actual build/runtime tools with no native equivalent.

### 2. QUALITY GATES — MANDATORY BEFORE "DONE"

You are NOT done until ALL of these pass:
- ✅ Zero IDE diagnostics (errors AND warnings)
- ✅ All tests pass
- ✅ No linting errors
- ✅ You reviewed your own diff and found nothing wrong

**SINGLE-PASS COMPLETION IS FORBIDDEN.** Run the verification loop. Actually check.

### 3. SUBAGENTS ARE BLIND — YOU MUST PASS CONTEXT

Subagents DO NOT SEE BY DEFAULT:
- This CLAUDE.md file
- The project CLAUDE.md file
- Your conversation history
- Files you've read

**YOU MUST INCLUDE IN EVERY SUBAGENT PROMPT:**
1. ALL CLAUDE.md content. This is non-negotiable. COPY IT ALL.
2. Actual file contents (not just paths)
3. Complete task context
4. Expected output format

**If a subagent produces bad output, it's because YOU didn't give it proper context.**

### 4. CONTEXT LIMIT — 85% = STOP

At 85% context: STOP. Write handoff to CLAUDE.md. Do NOT start new tasks. Do NOT "finish quickly."

### 5. SESSION STATE

Maintain `## Session State` in project CLAUDE.md during work. **DELETE IT** when task fully completes.

### 6. PLANS LOCATION

`./doc/plans/PLAN-XXXX-description.md` — **NOT** `~/.claude/plans/`. This overrides system defaults.

### 7. DEPENDENCIES — PACKAGE MANAGERS ONLY

**NEVER** manually edit package.json, Cargo.toml, pyproject.toml, go.mod, etc.
**ALWAYS** use: `npm install`, `cargo add`, `uv add`, `go get`

---

## Agent Orchestration

### When to Delegate

- **USE SUBAGENTS** when task matches their specialty (don't do everything yourself)
- **`tech-lead`** for complex multi-domain projects needing coordination
- **Senior agents (opus)**: Architecture, debugging, design patterns, code review, strategic decisions
- **Standard agents (sonnet)**: Implementation, boilerplate, well-defined routine tasks

### Parallel vs Sequential

**PARALLEL** (spawn multiple agents at once):
- Independent files/components with no shared dependencies
- Separate analyses or reviews
- Research tasks that don't depend on each other

**SEQUENTIAL** (wait for previous to complete):
- Task B needs Task A's output
- Shared state or resources
- Order matters (schema before data, interface before implementation)

### REMINDER: Subagents Are Blind

Every subagent prompt MUST include: (1) all CLAUDE.md content, (2) actual file contents, (3) complete context. **No exceptions.**

---

## Code Quality

**Size Limits:** Files 400 lines | Functions 50 lines | Classes 300 lines — refactor if exceeded

**Before Commit:** Zero diagnostics | Tests pass | Linter clean | Formatted

**Avoid:** Empty catch blocks | Magic numbers | `any` in TypeScript | Commented-out code | Console.log in production
