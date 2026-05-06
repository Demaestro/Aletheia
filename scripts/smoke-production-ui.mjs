import { mkdirSync } from "node:fs";
import { chromium } from "playwright";

const writableTmp = "/tmp/aletheia-playwright";
mkdirSync(writableTmp, { recursive: true });
process.env.TMPDIR = writableTmp;
process.env.TMP = writableTmp;
process.env.TEMP = writableTmp;

const url = process.env.ALETHEIA_PREVIEW_URL ?? "http://127.0.0.1:4182/";
const browser = await chromium.launch({ headless: true });
const page = await browser.newPage({ viewport: { width: 1440, height: 960 } });
const consoleErrors = [];
let seenText = "";

page.on("console", (message) => {
  if (message.type() === "error") consoleErrors.push(message.text());
});
page.on("pageerror", (error) => consoleErrors.push(error.message));

await page.goto(url, { waitUntil: "domcontentloaded", timeout: 15000 });
const openOperator = page.getByRole("button", { name: /open operator console/i });
if (await openOperator.count()) {
  await openOperator.click();
}

await page.getByText("Bible command desk").waitFor({ timeout: 10000 });
await page.getByText("Program monitor").waitFor({ timeout: 10000 });
await page.getByText("Command lane").waitFor({ timeout: 10000 });
await page.getByText("Preview and live output.").waitFor({ timeout: 10000 });
seenText += `\n${await page.locator("body").innerText()}`;

await page.locator("button").filter({ hasText: "Integrations" }).click();
await page.getByRole("button", { name: /save config/i }).first().waitFor({ timeout: 10000 });
await page.getByRole("button", { name: /export booth pack/i }).click();
await page.getByText("Booth compatibility pack", { exact: true }).waitFor({ timeout: 10000 });
await page.getByText("Recent delivery receipts").waitFor({ timeout: 10000 });
seenText += `\n${await page.locator("body").innerText()}`;

await page.locator("button").filter({ hasText: "Health" }).click();
await page.getByText("Production readiness", { exact: true }).waitFor({ timeout: 10000 });
await page.getByText("Plugin signing", { exact: true }).waitFor({ timeout: 10000 });
await page.getByRole("button", { name: /run local rehearsal/i }).click();
await page.getByText("Local rehearsal runner", { exact: true }).first().waitFor({ timeout: 10000 });
await page.getByText("SQLite scripture index").first().waitFor({ state: "attached", timeout: 10000 });
await page.getByRole("button", { name: /^export$/i }).waitFor({ timeout: 10000 });
seenText += `\n${await page.locator("body").innerText()}`;

const bodyText = seenText.toLowerCase();
const required = [
  "vmix bridge",
  "booth compatibility pack",
  "bible command desk",
  "program monitor",
  "command lane",
  "recent delivery receipts",
  "production readiness",
  "secret vault",
  "plugin signing",
  "local rehearsal runner",
  "support bundle",
  "device"
];
const missing = required.filter((label) => !bodyText.includes(label));
if (missing.length) throw new Error(`Missing expected production UI labels: ${missing.join(", ")}`);
if (consoleErrors.length) throw new Error(`Console errors: ${consoleErrors.join(" | ")}`);

mkdirSync("output/playwright", { recursive: true });
await page.screenshot({ path: "output/playwright/aletheia-production-smoke.png", fullPage: true });
await browser.close();

console.log(`Production UI smoke passed at ${url}`);
