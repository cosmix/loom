# Probe p2-side-variants

Claude Code: 2.1.321 (Claude Code)

## Pane after the opening turn

````text

 ▐▛███▛█   Claude Code v2.1.321
▝▜██████▀  Sonnet 5.5 with low effort · <PLAN>
 ▝▝   ▝▝   <BASE>/project


❯ Reply with the single word READY.                                                                                                                                                                                         

● READY

✻ Brewed for 2s · done 8:43 AM

































                                                                                                                                                                                                           ○ low · /effort
───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────── loom plan: probe ─
❯                                  
────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────
  Model: Sonnet 5.5 | Ctx: 0 | ⎇ main | (+0,-0)
  ⏵⏵ auto mode on (shift+tab to cycle) · ← for agents
````

## Side process: max-turns-0

````text
argv:
<HOME>/.local/bin/claude --settings <BASE>/loom-home/plan/project-9326e412d1db/sessions/<SESSION-1>/settings.side-max-turns-0.json --setting-sources user,project --model sonnet --effort low --permission-mode auto --plugin-dir <BASE>/loom-home/plan/project-9326e412d1db/sessions/<SESSION-1>/plugin --strict-mcp-config -p --input-format stream-json --output-format stream-json --verbose --no-session-persistence --max-turns 0

stdin: {"type":"control_request","request_id":"probe-init-1","request":{"subtype":"initialize"}} (sent: Ok(())); no user message

initialize answered: Some(381.523458ms)
system/init: None
commands_changed after adding a skill: None
exit: Ok(ExitStatus(unix_wait_status(9))); descendants left running: 0

files added or changed under <copied HOME>/.claude/projects: 0

live spool byte-identical: true
live transcript byte-identical: true

stdout line kinds:
381.523458ms control_response/-

initialize commands: 117 entries; /loom or /loom-agents: []
system/init skills: 0 entries; /loom or /loom-agents: []
system/init slash_commands: 0 entries; /loom or /loom-agents: []

initialize commands:
<SKILL-2>
<SKILL-4>
<SKILL-5>
<SKILL-6>
<SKILL-7>
<SKILL-9>
<SKILL-10>
<SKILL-11>
<SKILL-12>
<SKILL-13>
<SKILL-14>
<SKILL-15>
<SKILL-16>
<SKILL-17>
<SKILL-18>
<SKILL-19>
<SKILL-20>
<SKILL-21>
probe-fork
probe-personal
probe-webfetch
<SKILL-24>
<SKILL-26>
<SKILL-27>
<SKILL-28>
<SKILL-29>
address
<SKILL-3>
distill
<SKILL-22>
rusure
<SKILL-30>
deep-research
loom-plan:probe
<PLUGIN-4>:adversarial-review
<PLUGIN-4>:cancel
<PLUGIN-4>:rescue
<PLUGIN-4>:result
<PLUGIN-4>:review
<PLUGIN-4>:setup
<PLUGIN-4>:status
<PLUGIN-4>:transfer
loom-plan:args
<PLUGIN-5>:<PLUGIN-5>
<PLUGIN-1>:building-<PLUGIN-8>-agents
<PLUGIN-12>:import
<PLUGIN-12>:market-research
<PLUGIN-12>:review
<PLUGIN-12>:score
<PLUGIN-12>:triage
design
design-sync
dataviz
update-config
verify
debug
code-review
simplify
batch
fewer-permission-prompts
doctor
loop
schedule
claude-api
workflow-authoring
run
run-skill-generator
plugin-authoring
advisor
agents
auto-mode-setup
autocompact
clear
color
compact
config
output-style
context
effort
fast
focus
heapdump
init
mcp
import
model
__remote-workflow
workflow-launch-exec
reload-plugins
reload-skills
rename
ultrareview
security-review
usage-credits
extra-usage
usage
insights
recap
skill-doctor
goal
design-consent
design-revoke
list-agents
team-onboarding
built-in-browser
chrome-browser
computer-use
<PLUGIN-2>:deep-research
docs
docx
google-workspace
import-memory
morning
pdf
pptx
skill-creator
xlsx

system/init skills:


system/init slash_commands:


stderr, last 40 lines:
````

## loom-ui enabled

````text
loom-ui is not installed (no ~/.claude/skills/loom-ui): the enabled sub-check is skipped; the disabled run is in p2-side-initialize
````
