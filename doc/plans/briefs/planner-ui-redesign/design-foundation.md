# design-foundation executor brief

Read common.md in this directory first. Model: `gpt-6.1-sol`; effort: `high`.

Stage working directory: `web`. Each worker receives a bounded subset and explicit
owned files from the orchestrator, not the whole stage. The source plan defines design,
acceptance and stage boundaries.

## Stage requirements

Implements Design sections 3, 4, 5, 9, 11, the doodle of section 8 and the frame of section 10 of doc/plans/IN_PROGRESS-PLAN-planner-ui-redesign.md. Read that plan's Design sections and "Stage design-foundation" first; the artifact linked under Draft status shows the target. The fonts (D7) are settled.
PATHS: working_dir is web/; brief paths are repo-relative.
KNOWLEDGE: conventions/web-dashboard-typography.md (theme tokens; chrome uses the body face; TerminalMark; Help uses operational illustrations; Help illustrations use real components), conventions/plan-web.md (CodeMirror lazy boundary; bundle rules), architecture/web-dashboard.md.
FONT: bun add @fontsource-variable/source-serif-4 @fontsource-variable/literata @fontsource-variable/piazzolla; import them from src/plan/main.tsx only; add a bundle rule so index.html never contains them. Code stays Commit Mono Cosmix through --font-mono (src/fonts.css).
REUSE: Logo (src/components/logo.tsx), ThemeToggle (src/aurora-ui/theme/ThemeToggle.tsx), ThemePicker (src/components/theme-picker.tsx, which gains a swatch prop; the dashboard keeps its stage-graph swatch), the help dialog and pinned scenes (src/components/help-dialog.tsx, help-scene.tsx), the ViewSwitch idiom (src/components/view-switch.tsx), Button. Nothing from components/graph/stage-graph or the terminal (plan.html bundle rules).
MOTION: every activity glyph is CSS animation only, and prefers-reduced-motion stops all of them; the doodle is fully drawn at rest.
AFTER THE WORKERS: node_modules/.bin/oxfmt; bun run build once so node scripts/bundle-check.mjs has a manifest; inspect the header, settings and help at 390px and 1280px in all four themes before the gate.
Use parallel subagents and skills to maximize performance.
MEMORY: record mistakes, decisions and surprises with loom memory immediately, subagents too; NEVER Claude Code auto-memory.

Worker restrictions in common.md govern executor actions. Instructions above about
verification, git, memory and dispatch belong to the orchestrator or daemon.

## Stage file boundary (repository-relative)

- web/package.json
- web/bun.lock
- web/scripts/bundle-rules.json
- web/src/plan/main.tsx
- web/src/plan/plan.css
- web/src/plan/header/**
- web/src/plan/header-extras.tsx
- web/src/plan/ui/**
- web/src/plan/settings/**
- web/src/plan/help/**
- web/src/components/theme-picker.tsx
- web/src/components/theme-picker.test.tsx
- web/src/plan/checklist-view.tsx
- web/src/plan/shell.tsx
