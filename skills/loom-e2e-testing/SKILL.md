---
name: loom-e2e-testing
description: End-to-end testing for web applications with Playwright, Cypress, Selenium, and Puppeteer. Use for setting up E2E tests, debugging failures, improving reliability, and implementing browser automation with Page Object Model, selector strategies, network interception, visual regression, and flaky-test prevention.
triggers:
  - e2e
  - e2e testing
  - end-to-end
  - end-to-end tests
  - Playwright
  - Cypress
  - Selenium
  - Puppeteer
  - Page Object Model
  - page object
  - test fixtures
  - selectors
  - locator
  - locators
  - getByRole
  - data-testid
  - auto-wait
  - async tests
  - network interception
  - route mocking
  - visual regression
  - visual testing
  - screenshot
  - trace viewer
  - flaky tests
  - flakiness
  - browser testing
  - browser automation
  - UI test
  - UI testing
  - acceptance test
  - smoke test
  - storage state
  - test isolation
---

# E2E Testing

## Overview

Browser E2E validates full user journeys. This file owns **selectors, auto-waiting, network interception, POM, browser flakiness, and trace/video debugging** (Playwright-first, Cypress noted where it differs). E2E is the *tip* of the pyramid — keep it thin and reserve it for critical paths; push logic down to unit/integration (`loom-test-strategy`). For test-double taxonomy and AAA see `loom-testing`.

## Framework choice

Default to **Playwright** for new suites: true multi-browser (incl. WebKit/Safari), multi-tab/origin, out-of-process network interception, built-in parallelism and trace viewer, and non-JS bindings (Python/.NET/Java). Choose **Cypress** only when the team is already invested or wants its live time-travel debugger; its in-browser model limits multi-tab, cross-origin, and true WebKit. Selenium/Puppeteer: legacy or Chrome-only automation, not greenfield E2E.

```bash
npm init playwright@latest         # scaffolds config + CI + browsers
```

```typescript
// playwright.config.ts — the load-bearing knobs
export default defineConfig({
  testDir: "./e2e",
  fullyParallel: true,                       // parallel within files too
  forbidOnly: !!process.env.CI,              // fail CI if a .only slips in
  retries: process.env.CI ? 2 : 0,           // retry ONLY to surface flakes
  reporter: [["html"], ["junit", { outputFile: "results.xml" }]],
  use: {
    baseURL: "http://localhost:3000",
    trace: "on-first-retry",                 // trace the retry, not every run
    screenshot: "only-on-failure",
    video: "retain-on-failure",
  },
  webServer: {                                // boots the app, waits for it
    command: "npm run dev",
    url: "http://localhost:3000",
    reuseExistingServer: !process.env.CI,
  },
  projects: [
    { name: "chromium", use: { ...devices["Desktop Chrome"] } },
    { name: "firefox",  use: { ...devices["Desktop Firefox"] } },
    { name: "webkit",   use: { ...devices["Desktop Safari"] } },
  ],
});
```

⚠ `retries` exists to **diagnose** flakes (a test that passes only on retry is flaky and must be fixed), never to hide them — a retry-masked bug ships. `webServer` is why `page.goto` doesn't race the server boot; without it you get connection-refused flakes.

## Selectors — user-facing first

Priority, best → worst. Higher options survive refactors and assert accessibility for free; CSS/XPath couple tests to DOM structure and shatter on markup changes.

1. **Role + accessible name** — `getByRole("button", { name: "Sign in" })`
2. **Label / placeholder / text** — `getByLabel("Email")`, `getByText("Welcome")`
3. **`data-testid`** — for elements with no stable role/text
4. CSS — only for structural selection (`.modal-content`)
5. **XPath — avoid** (brittle, unreadable)

```typescript
page.getByRole("textbox", { name: "Email" });
page.getByLabel("Password");
page.getByTestId("product-card-123");           // add data-testid in the component
// chain/filter instead of nth-index (index shifts → flake)
page.getByTestId("product-list")
    .getByRole("listitem").filter({ hasText: "Widget" })
    .getByRole("button", { name: "Add to cart" });
```

⚠ Never select by index (`.nth(2)`, `items[2]`) or auto-generated CSS-module class hashes — both change silently. Add `data-testid` at the component and strip it in prod builds (`babel-plugin-react-remove-properties`). Cypress: use `@testing-library/cypress` (`cy.findByRole`) for the same priority.

## Auto-waiting vs explicit waits

Playwright locator actions **auto-wait** for the element to be attached, visible, stable, and enabled before acting, and web-first assertions (`expect(locator).toBeVisible()`) **retry** until they pass or time out. This eliminates almost all manual waiting.

```typescript
// RIGHT — assert the condition; it retries internally
await page.getByRole("button", { name: "Submit" }).click();
await expect(page.getByText("Success")).toBeVisible();

// WRONG — arbitrary sleep: flaky if slow, wasteful if fast
await page.click("#submit");
await page.waitForTimeout(2000);                 // ⛔ never
```

Use explicit waits only for things auto-wait can't see — navigation and network:

```typescript
await page.waitForURL("/dashboard");             // after a click that navigates
await page.getByTestId("spinner").waitFor({ state: "hidden" });
```

⚠ `waitForTimeout` is banned in real suites — it's the #1 source of both flakiness and slowness. `waitForLoadState("networkidle")` is discouraged by Playwright (racy on apps that poll); wait for a concrete UI condition instead. Cypress auto-retries assertions/queries but **not** raw values — re-query, don't cache elements.

## Network interception

Stub the network to make E2E deterministic and to reach error states the UI can't otherwise hit. Register the route **before** the navigation that triggers it.

```typescript
// Deterministic data — no dependence on backend state
await page.route("**/api/products", (route) =>
  route.fulfill({ status: 200, contentType: "application/json",
                  body: JSON.stringify([{ id: 1, name: "Test Widget" }]) }));

// Force an error path
await page.route("**/api/checkout", (route) => route.fulfill({ status: 500 }));

// Assert a request happened (wait for it, don't sleep)
const resp = page.waitForResponse("**/api/order");
await page.getByRole("button", { name: "Place order" }).click();
expect((await resp).status()).toBe(201);
```

⚠ Use glob/regex (`**/api/x`) — exact strings miss query params and absolute vs relative URLs. Decide deliberately: **stub** for speed/determinism/error-injection; hit the **real** backend for a handful of true full-stack smoke tests. Cypress uses `cy.intercept()` with an alias + `cy.wait("@alias")`.

## Page Object Model

Encapsulate page structure so a UI change touches one file, not fifty tests. Expose **user intentions** (`login(email, pw)`), not raw locators.

```typescript
// e2e/pages/LoginPage.ts
export class LoginPage {
  constructor(private page: Page) {}
  readonly email = () => this.page.getByLabel("Email");
  readonly submit = () => this.page.getByRole("button", { name: "Sign in" });
  async goto()  { await this.page.goto("/login"); }
  async login(email: string, pw: string) {
    await this.email().fill(email);
    await this.page.getByLabel("Password").fill(pw);
    await this.submit().click();
  }
}
```

⚠ Keep **assertions out of page objects** (except small `expect` helpers) — a POM describes the page; the test owns the verification. Prefer Playwright **fixtures** over a monolithic `App` object for composing pages/state; they scope setup/teardown automatically.

## Fixtures & auth (speed)

Logging in through the UI on every test is the biggest E2E time sink. Authenticate **once**, save `storageState`, and reuse it — cutting minutes off a suite.

```typescript
// global-setup: log in once, persist cookies/localStorage
await page.goto("/login"); /* ...fill+submit... */
await page.context().storageState({ path: "e2e/.auth/user.json" });

// then in config or a fixture:
use: { storageState: "e2e/.auth/user.json" }
```

Custom fixtures give per-test isolated data with automatic cleanup:

```typescript
export const test = base.extend<{ user: User }>({
  user: async ({}, use) => {
    const u = await prisma.user.create({ data: UserFactory.create() });
    await use(u);
    await prisma.user.delete({ where: { id: u.id } });   // teardown always runs
  },
});
```

⚠ Even faster: seed state via the **API request context** rather than clicking through setup — see hybrid pattern below.

## Preventing browser flakiness

General flaky-test theory (clock, RNG, shared state) and the full cause table are in `loom-test-strategy`. Browser-specific rules:

- **Wait for conditions, never timeouts** — covered above; this is 80% of E2E flake.
- **Select by role/text/testid, never index** — DOM order shifts.
- **Isolate per test** — Playwright gives each test a fresh browser context (clean cookies/storage) by default; don't defeat it with shared global state. Mint unique data (`test-${Date.now()}@x.com`).
- **Freeze time & animations** for anything time- or motion-sensitive:

```typescript
await page.clock.setFixedTime(new Date("2024-01-15T10:00:00Z"));
await page.emulateMedia({ reducedMotion: "reduce" });
```

- **Broken-window rule:** fix or quarantine a flaky test the day it appears; a tolerated flake trains the team to ignore red.

## Debugging: trace, video, screenshot

```bash
npx playwright test --trace on      # force a trace for a run
npx playwright show-trace trace.zip # timeline, DOM snapshots, network, console
npx playwright test --debug         # step through with Inspector
```

Configure artifacts on failure only (`trace: "on-first-retry"`, `video/screenshot: retain/only-on-failure` — see config) so green runs stay fast. The **trace viewer** (time-travel DOM + network + console per action) is the fastest way to root-cause a CI-only failure — wire trace upload into CI artifacts. `await page.pause()` opens the Inspector mid-test for local poking.

## Visual regression

```typescript
await expect(page).toHaveScreenshot("dashboard.png", {
  mask: [page.getByTestId("timestamp")],   // hide dynamic regions
  maxDiffPixelRatio: 0.01,
});
```

⚠ Screenshots are **OS/font/GPU-dependent** — generate and compare baselines in the *same* container you run CI in (`--update-snapshots` locally on a Mac then failing in Linux CI is the classic trap). Disable animations, mask dynamic content (dates, avatars, ads), and pin the viewport. Treat baselines as reviewed artifacts, not auto-accepted.

## Hybrid API + UI

Do setup and teardown over HTTP (fast, reliable); reserve the browser for the user-facing assertion. Cuts runtime and removes setup-related flake.

```typescript
test("cart shows seeded item", async ({ page, request }) => {
  await request.post("/api/cart/add", { data: { productId: "123", quantity: 2 } });
  await page.goto("/cart");
  await expect(page.getByTestId("cart-item")).toHaveCount(1);
  await expect(page.getByTestId("quantity")).toHaveText("2");
});
```

## Cross-browser scope

Run the full matrix (Chromium/Firefox/WebKit + mobile viewports) on **critical paths only**; run the rest on Chromium to keep CI fast. Skip project-specifically rather than duplicating tests:

```typescript
test.describe("Admin panel", () => {
  test.skip(({ browserName }) => browserName !== "chromium");  // Chrome-only
});
```

## Smoke suite

A tiny must-pass set gating every release — homepage renders, auth works, key APIs return < 500:

```typescript
test("user can sign in", async ({ page }) => {
  await page.goto("/login");
  await page.getByLabel("Email").fill("user@example.com");
  await page.getByLabel("Password").fill("password123");
  await page.getByRole("button", { name: "Sign in" }).click();
  await expect(page).toHaveURL("/dashboard");
});
```

## Verify before done

- [ ] Selectors are role/label/text/testid — no index, XPath, or generated-class-hash selectors
- [ ] Zero `waitForTimeout`/`sleep`; waits target a condition, URL, or response
- [ ] Auth via `storageState`; per-test data is unique; cleanup in fixture teardown
- [ ] Network stubbed where determinism/error-injection is needed; real backend only for true smoke tests
- [ ] `retries` used to *surface* flakes, not mask them; any retry-only pass is triaged
- [ ] trace/video/screenshot retained on failure and uploaded as CI artifacts
- [ ] E2E limited to critical journeys; logic-level cases pushed down (`loom-test-strategy`)
- [ ] Visual baselines generated in the CI container, with dynamic regions masked
- [ ] No `.only` left in the suite (`forbidOnly` on in CI)
