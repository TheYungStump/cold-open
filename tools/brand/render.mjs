// Renders the brand images (docs/specs/release.md §12) from card.html with
// Playwright's Chromium, as listed in images.json:
//
//   npm install --no-save --no-package-lock playwright@1.48.2
//   npx playwright install chromium
//   node tools/brand/render.mjs [name ...]
//
// With names, only those images. Each image's `out` is relative to the
// repository's root. The screenshots it uses come from
//   COLDOPEN_SHOT_SCRIPT=tools/brand/the-night-ferry.fountain \
//     cargo run --release -p coldopen-app --example screenshots -- tools/brand/shots
import { chromium } from "playwright";
import { readFileSync, mkdirSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, "..", "..");
const images = JSON.parse(readFileSync(join(here, "images.json"), "utf8"));
const only = process.argv.slice(2);

const browser = await chromium.launch();
const page = await browser.newPage({ deviceScaleFactor: 1 });
for (const img of images) {
  if (only.length && !only.includes(img.name)) continue;
  const { out, name, ...card } = img;
  await page.setViewportSize({ width: card.w, height: card.h });
  const url = pathToFileURL(join(here, "card.html")).href + "#" + encodeURIComponent(JSON.stringify(card));
  await page.goto(url);
  await page.waitForSelector("body[data-ready='1']");
  const file = join(root, out);
  mkdirSync(dirname(file), { recursive: true });
  await page.locator("#card").screenshot({ path: file });
  console.log(`${name}: ${out} (${card.w}x${card.h})`);
}
await browser.close();
