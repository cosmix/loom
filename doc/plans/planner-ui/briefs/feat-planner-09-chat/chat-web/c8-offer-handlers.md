# chat-web C8: authoring offer handlers

Codex unit (gpt-6.1-terra, effort high). Working dir: web/.

## Files

- Owns: `src/plan/chat/offer-handlers.ts`; `src/plan/chat/offer-handlers.test.ts`
- May read: `src/plan/offers/actions.ts (plan 2: registerOfferAction, OfferView)`; `src/plan/api/socket.ts (plan 2: send)`

## Entry points

export function registerAuthoringOffers(): () => void

## Steps

1. Register one handler per authoring kind (start_vision, write_section, add_section_to_vision, start_metaplan, write_plan, add_plan_to_metaplan, refine_spec, new_metaplan_round, fix_vision_table) through registerOfferAction; each sends { type: "chat", command: "start", client_id, offer: offer.id, text: null, attachments: [] }.
2. Return one function that calls every unregister.
3. Tests: every authoring kind registered; handler sends start with the offer id; pressure_test and start_run are not registered.

## Done

Three tests pass.

Proof (the orchestrator runs it after you): `node_modules/.bin/vitest run src/plan/chat/offer-handlers.test.ts`

## Traps

Never edit src/plan/offers/actions.ts; it belongs to plan 2. client_id is crypto.randomUUID().

Rules: erasable TypeScript only (no parameter properties, enums or namespaces; tsconfig sets
erasableSyntaxOnly). Import z only from @/api/zod. No dangerouslySetInnerHTML. Do not run oxfmt;
the orchestrator formats after you. Keep the file under 360 lines. Your own check is
node_modules/.bin/tsc -b --noEmit from web/. Do not run git. Do not touch any dot-loom or dot-work
path.
