/** Click through human decisions until a spent pip renders as a filled circle. */
import { chromium } from 'playwright';

const shot = process.argv[2] ?? '/tmp/pips.png';
const browser = await chromium.launch({ executablePath: '/opt/pw-browsers/chromium' });
const page = await browser.newPage({ viewport: { width: 1500, height: 1150 } });
const errors = [];
page.on('pageerror', (e) => errors.push(e.message));
await page.goto('http://localhost:5173/', { waitUntil: 'networkidle' });
await page.waitForTimeout(1500);

for (let i = 0; i < 60; i++) {
  const circles = await page.locator('.pip-circle').count();
  const filled = await page.locator('.pip-circle circle[fill="currentColor"]').count();
  if (filled > 0) {
    console.log(`circles=${circles} filled=${filled} after ${i} decisions`);
    await page.screenshot({ path: shot, clip: { x: 0, y: 60, width: 1000, height: 240 } });
    console.log('errors:', errors.length ? errors : 'none');
    await browser.close();
    process.exit(0);
  }
  const buttons = page.locator('.action-list button:not(:disabled)');
  if ((await buttons.count()) === 0) {
    await page.waitForTimeout(350);
    continue;
  }
  // Prefer a pip-spending action over ending the turn, to reach a fill.
  const n = await buttons.count();
  let clicked = false;
  for (let b = 0; b < n; b++) {
    const text = await buttons.nth(b).innerText();
    if (!/End turn|Pass/i.test(text)) {
      await buttons.nth(b).click();
      clicked = true;
      break;
    }
  }
  if (!clicked) await buttons.first().click();
  await page.waitForTimeout(250);
}
console.log('never saw a filled pip');
console.log('errors:', errors.length ? errors : 'none');
await browser.close();
process.exit(1);
