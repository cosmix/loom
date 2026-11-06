# Spec Scope Invention
>

>

## Spec sections invented product features the vision never asked for

**What happened**: Writing the feat-planner spec sections from the vision, an agent turned one sentence about a returning author ('revise the spec or add plans in light of what was built') into two done-state offers, 'Compare spec with build' (with a new loom-plan:retro skill) and 'New metaplan round', and later plans implemented both. The user never asked for either and rejected the first.
**Why**: A user description in the vision was read as a feature request, and spec sections were treated as license to add UI and skills.
**Prevention**: A spec section adds a user-facing action, button or skill only when the vision names it; a user story the vision describes gets an open question in the section, not an invented feature. Mark any such addition '(My call.)' and raise it with the user before plans build it.
**Fix**: The plan planner-follow-ups removed both offers and the `loom-plan:retro` skill. Spec and build are aligned inside each run by integration-verify's spec-conformance duty ([the Product Sections block](../architecture/signal-generation.md#product-sections-block-integration-verify-only)); follow-up work is the Built page's single `Plan follow-ups` offer, which appends entries under `## Follow-ups` in the feature's `index.md` ([Built page](../architecture/planner-feature-screens.md#built-page)).
