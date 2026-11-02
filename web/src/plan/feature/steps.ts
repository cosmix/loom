/**
 * The server's step numbers (`planning::checklist::Step`). The names arrive with the checklist
 * (`ChecklistView.steps`, `StepView.name`); the web only needs the numbers to pick a step.
 */
export const STEP = { vision: 1, product: 2, metaplan: 3, plans: 4, pressure: 5, run: 6 } as const;
