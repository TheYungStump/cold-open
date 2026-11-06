// The download page and the started pages in a browser (docs/specs/site.md
// §5, §7), on a site built from tools/site_release_fixture.json, which has
// every build. Playwright's Chromium, with the computer each case describes
// put in place before the page's scripts run.
//
//   node tools/site_browser.mjs http://127.0.0.1:8000/cold-open/
//
// Exit status 1 when a case fails.
import { chromium } from "playwright";

const base = process.argv[2];
const UA = {
  winChrome: "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36",
  winFirefox: "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:132.0) Gecko/20100101 Firefox/132.0",
  macChrome: "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36",
  macSafari: "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.1 Safari/605.1.15",
  linuxFirefox: "Mozilla/5.0 (X11; Linux x86_64; rv:132.0) Gecko/20100101 Firefox/132.0",
  linuxChrome: "Mozilla/5.0 (X11; Linux aarch64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36",
  android: "Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Mobile Safari/537.36",
  iphone: "Mozilla/5.0 (iPhone; CPU iPhone OS 18_1 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.1 Mobile/15E148 Safari/604.1",
};

// What each case's browser says, and what the page must recommend (a
// build id, or null), and which group it lists first.
const CASES = [
  { name: "Windows x64, Chrome", ua: UA.winChrome, platform: "Win32", touch: 0,
    uad: { platform: "Windows", mobile: false, hints: { architecture: "x86", bitness: "64" } },
    want: "windows-x64", first: "windows" },
  { name: "Windows ARM, Edge", ua: UA.winChrome + " Edg/130.0.0.0", platform: "Win32", touch: 0,
    uad: { platform: "Windows", mobile: false, hints: { architecture: "arm", bitness: "64" } },
    want: "windows-arm64", first: "windows" },
  { name: "Windows, Firefox (no client hints)", ua: UA.winFirefox, platform: "Win32", touch: 0,
    uad: null, want: null, first: "windows", tell: true },
  { name: "macOS, Apple chip, Chrome", ua: UA.macChrome, platform: "MacIntel", touch: 0,
    uad: { platform: "macOS", mobile: false, hints: { architecture: "arm", bitness: "64" } },
    want: "macos-universal", first: "macos" },
  { name: "macOS, Intel, Chrome", ua: UA.macChrome, platform: "MacIntel", touch: 0,
    uad: { platform: "macOS", mobile: false, hints: { architecture: "x86", bitness: "64" } },
    want: "macos-universal", first: "macos" },
  { name: "macOS, Safari (says Intel on both chips)", ua: UA.macSafari, platform: "MacIntel", touch: 0,
    uad: null, want: "macos-universal", first: "macos" },
  { name: "Linux x86-64, Firefox", ua: UA.linuxFirefox, platform: "Linux x86_64", touch: 0,
    uad: null, want: "linux-x64-appimage", first: "linux" },
  { name: "Linux ARM64, Chrome", ua: UA.linuxChrome, platform: "Linux aarch64", touch: 0,
    uad: { platform: "Linux", mobile: false, hints: { architecture: "arm", bitness: "64" } },
    want: "linux-arm64-appimage", first: "linux" },
  { name: "Android phone", ua: UA.android, platform: "Linux armv8l", touch: 5,
    uad: { platform: "Android", mobile: true, hints: { architecture: "", bitness: "" } },
    want: null, phone: true },
  { name: "iPhone", ua: UA.iphone, platform: "iPhone", touch: 5, uad: null, want: null, phone: true },
  { name: "iPad (Safari says Mac)", ua: UA.macSafari, platform: "MacIntel", touch: 5,
    uad: null, want: null, phone: true },
];

function pretend(c) {
  // Runs in the page before its scripts: the case's navigator.
  return `
    (() => {
      const def = (k, v) => Object.defineProperty(Navigator.prototype, k, { get: () => v, configurable: true });
      def("platform", ${JSON.stringify(c.platform)});
      def("maxTouchPoints", ${c.touch});
      ${c.uad
        ? `def("userAgentData", { platform: ${JSON.stringify(c.uad.platform)}, mobile: ${c.uad.mobile},
             brands: [], getHighEntropyValues: () => Promise.resolve(${JSON.stringify(
               Object.assign({ platform: c.uad.platform }, c.uad.hints))}) });`
        : `def("userAgentData", undefined);`}
    })();`;
}

const browser = await chromium.launch();
let failures = 0;
const fail = (msg) => { failures++; console.log("FAIL " + msg); };

// The recommendation, per computer (§5.2).
for (const c of CASES) {
  const context = await browser.newContext({ userAgent: c.ua, viewport: c.phone ? { width: 390, height: 844 } : { width: 1280, height: 900 } });
  await context.addInitScript(pretend(c));
  const page = await context.newPage();
  await page.goto(base + "download/?from=writers");
  await page.waitForTimeout(150);
  const got = await page.evaluate(() => {
    const box = document.getElementById("recommend");
    const link = document.getElementById("recommend-link").getAttribute("href");
    const first = document.querySelector("#downloads > section");
    return {
      shown: !box.hidden,
      href: link,
      first: first ? first.dataset.os : null,
      phone: !document.getElementById("not-a-computer").hidden,
      tell: !document.getElementById("how-to-tell").hidden,
      firstLink: document.querySelector("a[data-build]").getAttribute("href"),
    };
  });
  const want = c.want ? `download/started/${c.want}/writers/` : null;
  if (c.want && (!got.shown || !got.href.endsWith(want))) fail(`${c.name}: recommended ${got.shown ? got.href : "nothing"}, wanted ${want}`);
  if (!c.want && got.shown) fail(`${c.name}: recommended ${got.href}, wanted nothing`);
  if (c.first && got.first !== c.first) fail(`${c.name}: listed ${got.first} first, wanted ${c.first}`);
  if (!!c.phone !== got.phone) fail(`${c.name}: the "not a computer" note is ${got.phone ? "shown" : "hidden"}`);
  if (!!c.tell !== got.tell) fail(`${c.name}: "how to tell" is ${got.tell ? "shown" : "hidden"}`);
  if (!got.firstLink.endsWith("/writers/")) fail(`${c.name}: ?from=writers didn't reach the links (${got.firstLink})`);
  console.log(`${failures ? "…" : "ok"} ${c.name}: ${got.shown ? got.href : "no recommendation"}`);
  await context.close();
}

// Without JavaScript: the whole list, no recommendation, plain links (§5.2).
{
  const context = await browser.newContext({ javaScriptEnabled: false });
  const page = await context.newPage();
  await page.goto(base + "download/?from=ai");
  const hidden = await page.locator("#recommend").isHidden();
  const links = await page.locator("a[data-build]").count();
  const first = await page.locator("a[data-build]").first().getAttribute("href");
  if (!hidden) fail("JS off: a recommendation shows");
  if (links !== 13) fail(`JS off: ${links} downloads listed, wanted 13`);
  if (first.endsWith("/ai/")) fail("JS off: links carry from without JavaScript");
  console.log("ok JS off: the full list, no recommendation");
  await context.close();
}

// The started pages start the download with JavaScript on or off, and with
// the analytics beacon blocked (§5.3).
for (const js of [true, false]) {
  const context = await browser.newContext({ javaScriptEnabled: js });
  const page = await context.newPage();
  let asked = null;
  await page.route("https://static.cloudflareinsights.com/**", (r) => r.abort());
  await page.route("https://github.com/**/releases/download/**", (r) => {
    asked = r.request().url();
    return r.fulfill({ status: 200, contentType: "application/octet-stream",
      headers: { "Content-Disposition": "attachment" }, body: "x" });
  });
  page.on("download", () => {});
  await page.goto(base + "download/started/windows-x64/writers/");
  for (let i = 0; i < 40 && !asked; i++) await page.waitForTimeout(100);
  if (!asked || !asked.endsWith("-windows-x64-setup.exe")) fail(`started page, JavaScript ${js ? "on" : "off"}: the download didn't start (${asked})`);
  else console.log(`ok started page, JavaScript ${js ? "on" : "off"}: ${asked.split("/").pop()}`);
  await context.close();
}

await browser.close();
console.error(`${CASES.length} computers, ${failures} failure(s)`);
process.exit(failures ? 1 : 0);
