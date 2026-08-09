/** Drive the play tab: act, undo, and check the game rewinds exactly. */
import { chromium } from 'playwright';

const shot = process.argv[2] ?? '/tmp/undo.png';
const browser = await chromium.launch({ executablePath: '/opt/pw-browsers/chromium' });
const page = await browser.newPage({ viewport: { width: 1500, height: 1150 } });
const errors = [];
page.on('pageerror', (e) => errors.push(e.message));
await page.goto('http://localhost:5173/', { waitUntil: 'networkidle' });
await page.waitForTimeout(1500);

const status = () => page.locator('.status-line').first().innerText();
const logLen = () => page.locator('.log li').count();
const undoBtn = page.locator('.undo-btn');

console.log('undo disabled before any action:', await undoBtn.isDisabled());

// Take decisions until the pip circles appear, then one more.
for (let i = 0; i < 12; i++) {
  const before = await status();
  if (/thinking/.test(before)) {
    await page.waitForTimeout(400);
    continue;
  }
  const circles = await page.locator('.pip-circle').count();
  if (circles > 0) {
    console.log('pip circles:', circles, '— filled:', await page.locator('.pip-circle circle[fill="currentColor"]').count());
    break;
  }
  const btn = page.locator('.action-list button').first();
  if ((await btn.count()) === 0) {
    await page.waitForTimeout(400);
    continue;
  }
  await btn.click();
  await page.waitForTimeout(300);
}

// Now spend one pip and check a circle fills, then undo and check it empties.
const spent = () => page.locator('.pip-circle circle[fill="currentColor"]').count();
const beforeSpend = { status: await status(), log: await logLen(), spent: await spent() };
await page.locator('.action-list button').first().click();
await page.waitForTimeout(300);
const afterSpend = { status: await status(), log: await logLen(), spent: await spent() };

await undoBtn.click();
await page.waitForTimeout(300);
const afterUndo = { status: await status(), log: await logLen(), spent: await spent() };

console.log('before spend:', JSON.stringify(beforeSpend));
console.log('after spend: ', JSON.stringify(afterSpend));
console.log('after undo:  ', JSON.stringify(afterUndo));
console.log('undo restored status:', afterUndo.status === beforeSpend.status);
console.log('undo restored log:', afterUndo.log === beforeSpend.log);
console.log('undo restored pips:', afterUndo.spent === beforeSpend.spent);
console.log('errors:', errors.length ? errors : 'none');
await page.screenshot({ path: shot, clip: { x: 0, y: 60, width: 1000, height: 300 } });
await browser.close();
