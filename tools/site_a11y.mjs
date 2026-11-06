// Accessibility of the built website (docs/specs/site.md §6): axe-core's
// WCAG 2.2 A and AA rules on every page, once with the system in light mode
// and once in dark, through Playwright's Chromium. CI only; it needs
// `playwright` and `@axe-core/playwright` installed beside it.
//
//   node tools/site_a11y.mjs http://127.0.0.1:8000/cold-open/ docs/guide/book
//
// Exit status 1 when any page breaks a rule.
import { readdirSync, statSync } from "node:fs";
import { join, relative } from "node:path";
import { chromium } from "playwright";
import { AxeBuilder } from "@axe-core/playwright";

const [base, dir] = process.argv.slice(2);
const pages = [];
(function walk(d) {
  for (const name of readdirSync(d)) {
    const p = join(d, name);
    if (statSync(p).isDirectory()) walk(p);
    // The print page repeats every page; the 404 page is checked as any.
    else if (name.endsWith(".html") && name !== "print.html" && name !== "toc.html") {
      const rel = relative(dir, p).split("\\").join("/");
      // The started pages are one template: one stands for all, and
      // site_browser.mjs checks its download. download.html only moves on.
      if (rel.startsWith("download/started/") && rel !== "download/started/windows-x64/index.html") continue;
      if (rel === "download.html") continue;
      pages.push(rel);
    }
  }
})(dir);

const browser = await chromium.launch();
let failures = 0;
for (const scheme of ["light", "dark"]) {
  // A fresh context each time: no theme remembered, so mdBook follows the
  // system's scheme, as a first visit does.
  const context = await browser.newContext({ colorScheme: scheme });
  // A started page sends the browser to its file on GitHub. "204 No
  // Content" keeps the browser on the page; an aborted request sometimes
  // still counted as a navigation, and axe lost its page mid-check.
  await context.route("https://github.com/**/releases/download/**", (r) => r.fulfill({ status: 204 }));
  for (const page of pages) {
    const tab = await context.newPage();
    await tab.goto(base + page, { waitUntil: "load" });
    const result = await new AxeBuilder({ page: tab })
      .withTags(["wcag2a", "wcag2aa", "wcag21a", "wcag21aa", "wcag22aa"])
      .analyze();
    for (const v of result.violations) {
      failures++;
      console.log(`${scheme} ${page}: ${v.id} (${v.impact}) ${v.help}`);
      for (const n of v.nodes.slice(0, 3)) console.log(`    ${n.target.join(" ")}: ${n.failureSummary.split("\n").join(" ")}`);
    }
    await tab.close();
  }
  await context.close();
}
await browser.close();
console.error(`${pages.length} pages in light and dark mode, ${failures} violation(s)`);
process.exit(failures ? 1 : 0);
