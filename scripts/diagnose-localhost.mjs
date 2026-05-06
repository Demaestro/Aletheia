import { chromium } from "playwright";

const url = process.argv[2] ?? "http://127.0.0.1:5178";
const browser = await chromium.launch({ headless: true });
const page = await browser.newPage();
const logs = [];

page.on("console", (message) => {
  logs.push(`${message.type()}: ${message.text()}`);
});
page.on("pageerror", (error) => {
  logs.push(`pageerror: ${error.stack ?? error.message}`);
});
page.on("requestfailed", (request) => {
  logs.push(`requestfailed: ${request.url()} ${request.failure()?.errorText ?? ""}`);
});

try {
  const response = await page.goto(url, { waitUntil: "networkidle", timeout: 30_000 });
  await page.waitForTimeout(3_000);
  const title = await page.title().catch(() => "");
  const body = await page.locator("body").innerText().catch((error) => `BODY_READ_ERROR: ${error.message}`);
  const rootHtml = await page.locator("#root").innerHTML().catch((error) => `ROOT_READ_ERROR: ${error.message}`);

  console.log(`URL ${page.url()}`);
  console.log(`STATUS ${response?.status() ?? "no-response"}`);
  console.log(`TITLE ${title}`);
  console.log(`BODY_START\n${body.slice(0, 3000)}`);
  console.log(`ROOT_HTML_START\n${rootHtml.slice(0, 3000)}`);
  console.log(`LOGS_START\n${logs.join("\n")}`);
} finally {
  await browser.close();
}
