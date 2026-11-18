// J1 (every spine row opens real content) and J2 (a summary opens its passages and its source)
// of web/scripts/plan-journeys.mjs. J1 starts from the feature's landing, the product overview
// (feat-planner has a product spec; web/src/plan/feature/landing.ts).

import { readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";
import {
  ROOT,
  SECTION_5,
  SPEC_DIR,
  SPINE_SELECTOR,
  clickSpineRow,
  expandGroups,
  mainText,
  openSpine,
  pageState,
  pathOf,
  settle,
  shooter,
  spineHrefs,
  visit,
} from "./plan-journeys-kit.mjs";

// J1 --------------------------------------------------------------------------------------------

async function openRow(page, href, wide) {
  const nav = await openSpine(page);
  await expandGroups(nav);
  const link = nav.locator(`a[href=${JSON.stringify(href)}]`).first();
  await link.click();
  try {
    await page.waitForURL((u) => pathOf(u.href) === pathOf(href));
  } catch {
    throw new Error(`row ${href} landed on ${pathOf(page.url())}`);
  }
  if (!wide) return;
  // The URL changes before React commits the new row, so wait for the mark.
  await page
    .locator(`${SPINE_SELECTOR} a[href=${JSON.stringify(href)}][aria-current="page"]`)
    .first()
    .waitFor({ state: "attached", timeout: 5000 })
    .catch(() => {
      throw new Error(`row ${href} is not marked current`);
    });
}

export async function j1(env, width) {
  const { page, origin, outDir } = env;
  const shot = shooter(outDir, `J1-${width}`);
  await visit(page, origin, ROOT);
  try {
    await page.waitForURL((u) => u.pathname === `${ROOT}/product`);
  } catch {
    throw new Error(`${ROOT} landed on ${pathOf(page.url())}, not ${ROOT}/product`);
  }
  // The first row (Vision) is not the landing. Narrow, openRow does not wait for React to commit
  // the new route, so the first row must differ from the landing's text to count as opened.
  await pageState(page, `${ROOT}/product`, "");
  let prev = await mainText(page);
  const nav = await openSpine(page);
  await expandGroups(nav);
  const hrefs = await spineHrefs(nav);
  if (hrefs.length === 0) throw new Error("the spine has no rows");
  await shot(page, "spine");
  const fails = [];
  for (const href of hrefs) {
    try {
      await openRow(page, href, width >= 1024);
      await pageState(page, href, prev);
      prev = await mainText(page);
      env.routes.add(href);
      await shot(page, `row-${pathOf(href).split("/").slice(3).join("-") || "root"}`);
    } catch (error) {
      fails.push(`${width}px ${error.message.split("\n")[0]}`);
    }
  }
  if (fails.length) throw new Error(fails.join(" | "));
  return `${hrefs.length} of ${hrefs.length} rows open real content at ${width}px`;
}

// J2 --------------------------------------------------------------------------------------------

const SOURCE_ROOT = `${ROOT}/source/`;
const norm = (s) => s.toLowerCase().replace(/[^\p{L}\p{N}]+/gu, "");
const unlink = (md) => md.replace(/\[([^\]]*)\]\([^)]*\)/g, "$1");

function specCorpus(clone) {
  const dir = join(clone, SPEC_DIR);
  const files = readdirSync(dir).filter((name) => name.endsWith(".md"));
  return files.map((name) => norm(unlink(readFileSync(join(dir, name), "utf8")))).join(" ");
}

async function openFirstPassage(page) {
  const nested = '.concept-run [data-sentence-id][role="button"][aria-expanded="false"]';
  const any = '[data-sentence-id][role="button"][aria-expanded="false"]';
  let first = null;
  for (
    let step = 0;
    step < 8 && (await page.locator(".concept-passage").count()) === 0;
    step += 1
  ) {
    const target = (await page.locator(nested).count()) ? page.locator(nested) : page.locator(any);
    if ((await target.count()) === 0) throw new Error("no openable sentence on the product page");
    first ??= await target.first().getAttribute("data-focus");
    await target.first().click();
  }
  if ((await page.locator(".concept-passage").count()) === 0) {
    throw new Error("eight sentence clicks opened no source passage");
  }
  return first;
}

async function foldAll(page) {
  for (let i = 0; i < 12 && (await page.locator("button.concept-run-cap").count()); i += 1) {
    await page.locator("button.concept-run-cap").first().click();
  }
}

async function j2Passage(env) {
  const { page, origin, shot, clone } = env;
  await visit(page, origin, `${ROOT}/product`);
  const first = await openFirstPassage(page);
  await shot(page, "passage-open");
  const lines = await page.locator(".concept-passage").first().locator("p, li").allInnerTexts();
  const checked = lines.map(norm).filter((line) => line.length >= 20);
  const corpus = specCorpus(clone);
  const missing = checked.find((line) => !corpus.includes(line));
  if (checked.length === 0) throw new Error("the opened passage has no text");
  if (missing !== undefined)
    throw new Error(`passage text is not in ${SPEC_DIR}: ${missing.slice(0, 60)}`);
  await foldAll(page);
  await shot(page, "folded");
  const back = page.locator(`[data-focus=${JSON.stringify(first)}][role="button"]`);
  if ((await page.locator(".concept-passage").count()) || (await back.count()) === 0) {
    throw new Error("folding did not bring the sentence back");
  }
}

async function j2Views(env) {
  const { page, shot, clone } = env;
  const row = await clickSpineRow(page, (href) => href.endsWith("/product/section_05"));
  if (row === false) throw new Error("the spine has no section_05 row");
  await settle(page);
  await shot(page, "section-summary");
  const view = (name) => page.getByRole("link", { name, exact: true }).first();
  await view("Source").click();
  await page.waitForURL((u) => u.pathname.startsWith(SOURCE_ROOT));
  // Source stays inside the feature: the spine is still there.
  if ((await page.locator(SPINE_SELECTOR).count()) === 0) {
    throw new Error("the spine is gone on the source page");
  }
  const heading = readFileSync(join(clone, SECTION_5), "utf8").match(/^#\s+(.+)$/m)?.[1] ?? "";
  const want = unlink(heading).replace(/[*_`]/g, "").trim();
  await page.waitForFunction((text) => document.body.innerText.includes(text), want);
  await shot(page, "section-source");
  await view("Summary").click();
  await page.waitForURL((u) => !u.pathname.startsWith(SOURCE_ROOT));
  await pageState(page, "section_05 summary", "");
  await shot(page, "section-summary-again");
}

export async function j2(env) {
  await j2Passage(env);
  await j2Views(env);
  return "passage opened and folded, Summary and Source both open, the spine stays on Source";
}
