# Planner Visual Feedback Shortfall

> Visual asks reduced to DOM tests; shared-table summary expanded twice

## Visual feedback met by contract tests while the look stayed bland

**What happened**: planner-polish stage V2 was to restyle the Settings summary statistics (user issue 12). Its acceptance tests only checked that the Token totals region exists and the limit meters are gone; the stage passed and the user found the block as bland as before. The same review also found summary sentences that share one table each expanding into the same passage, so the table showed twice.
**Why**: A visual request was turned into DOM-presence criteria, so nothing tested the look; and the summary passage unit (a heading section per sentence) was never checked against sentences that cite the same section.
**Prevention**: A visual-quality item needs a design brief stating the concrete treatment (layout, type, colour, motion) and a screenshot check by the stage before completion. A summary expansion shows each source passage once: sentences in one paragraph that cite the same section open as one unit.
**Fix**: Pending: covered by doc/plans/PLAN-planner-follow-ups.md, polish stages.

## The chat sidebar redesign shipped without visible change

**What happened**: planner-polish V3 gave a fable worker with frontend-design the whole chat sidebar redesign (issue 15). The user then found the chat unchanged in look; the ⋯ menu still held End chat, the idle row still read 'Waiting for you', and New chat still started a session that spent 51.8k tokens on a 'no step chosen' opening before the user typed.
**Why**: The brief named the goal ('radically improve') without a concrete treatment or a before/after screenshot gate, and New chat's eager session start was outside the visual worker's files, so nobody owned its cost.
**Prevention**: A redesign starts from a mockup the user approves; the stage gates on screenshots matching it. A chat starts a model session only when the user sends the first message.
**Fix**: Pending: doc/plans/PLAN-planner-follow-ups.md stage 13.
