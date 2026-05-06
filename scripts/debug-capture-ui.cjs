const { chromium } = require("playwright");

(async () => {
  const browser = await chromium.launch({ headless: true });
  const page = await browser.newPage({ viewport: { width: 1440, height: 960 } });
  await page.goto("http://127.0.0.1:5178/", {
    waitUntil: "domcontentloaded",
    timeout: 20000,
  });
  const open = page.getByRole("button", { name: /open operator console/i });
  if (await open.count()) {
    await open.click();
  }
  await page.getByText("Bible command desk").waitFor({ timeout: 15000 });
  console.log(await page.locator("body").innerText());
  await browser.close();
})();
