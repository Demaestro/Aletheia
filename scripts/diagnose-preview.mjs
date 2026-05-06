import { chromium } from "playwright";

const browser = await chromium.launch({ headless: true });
const page = await browser.newPage();
const logs = [];

page.on("console", (message) => {
  logs.push(`[${message.type()}] ${message.text()}`);
});
page.on("pageerror", (error) => {
  logs.push(`[pageerror] ${error.stack || error.message}`);
});

const response = await page.goto("http://127.0.0.1:5178/", {
  waitUntil: "domcontentloaded",
  timeout: 30_000,
});
await page.waitForTimeout(5_000);

const bodyText = await page
  .locator("body")
  .innerText()
  .catch((error) => `BODY_ERROR ${error.message}`);
const rootHtml = await page
  .locator("#root")
  .evaluate((element) => element.innerHTML.slice(0, 2_000))
  .catch((error) => `ROOT_ERROR ${error.message}`);

console.log("STATUS", response?.status());
console.log("TITLE", await page.title());
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
