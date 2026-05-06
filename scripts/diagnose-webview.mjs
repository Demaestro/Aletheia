import { chromium } from "playwright";

const browser = await chromium.connectOverCDP("http://127.0.0.1:9222");
const context = browser.contexts()[0];
const page = context.pages()[0] ?? await context.newPage();
const logs = [];

page.on("console", (message) => {
  logs.push(`[${message.type()}] ${message.text()}`);
});
page.on("pageerror", (error) => {
  logs.push(`[pageerror] ${error.stack || error.message}`);
});
page.on("request", (request) => {
  logs.push(`[request] ${request.method()} ${request.url()}`);
});
page.on("response", (response) => {
  logs.push(`[response] ${response.status()} ${response.url()}`);
});
page.on("requestfailed", (request) => {
  logs.push(`[requestfailed] ${request.url()} ${request.failure()?.errorText ?? ""}`);
});

await page.waitForTimeout(2_000);

if (page.url() === "about:blank") {
  console.log("NAVIGATING_FROM_ABOUT_BLANK");
  await page.goto("http://127.0.0.1:5178/", {
    waitUntil: "domcontentloaded",
    timeout: 30_000,
  });
  await page.waitForTimeout(2_000);
}

const bodyText = await page
  .locator("body")
  .innerText()
  .catch((error) => `BODY_ERROR ${error.message}`);
const rootHtml = await page
  .locator("#root")
  .evaluate((element) => element.innerHTML.slice(0, 2_000))
  .catch((error) => `ROOT_ERROR ${error.message}`);
const state = await page.evaluate(() => ({
  href: location.href,
  title: document.title,
  readyState: document.readyState,
  rootExists: Boolean(document.getElementById("root")),
  rootChildCount: document.getElementById("root")?.childElementCount ?? null,
  bodyChildCount: document.body?.childElementCount ?? null,
  tauriInternals: Boolean(window.__TAURI_INTERNALS__),
}));

console.log("STATE", JSON.stringify(state, null, 2));
console.log("BODY_TEXT_START");
console.log(bodyText.slice(0, 2_000));
console.log("BODY_TEXT_END");
console.log("ROOT_HTML_START");
console.log(rootHtml);
console.log("ROOT_HTML_END");
console.log("LOGS_START");
console.log(logs.join("\n"));
console.log("LOGS_END");

await browser.close();
