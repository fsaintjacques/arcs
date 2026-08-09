/**
 * Check the undo reveal-barrier wiring end to end: undo works across plain
 * decisions, and is dead at the first decision after the chapter deal — a
 * chance node, the same code path as a battle roll.
 */
import { chromium } from 'playwright';

const browser = await chromium.launch({ executablePath: '/opt/pw-browsers/chromium' });
const page = await browser.newPage({ viewport: { width: 1500, height: 1150 } });
const errors = [];
page.on('pageerror', (e) => errors.push(e.message));
await page.goto('http://localhost:5173/', { waitUntil: 'networkidle' });
await page.waitForTimeout(1500);

const undoEnabled = async () => !(await page.locator('.undo-btn').isDisabled());
const chapter = async () =>
  Number((await page.locator('.status-line strong').first().innerText()).replace(/\D/g, ''));
const buttons = page.locator('.action-list button:not(:disabled)');

let sawUndoWork = false;
let verdict = null;

for (let i = 0; i < 300 && verdict === null; i++) {
  if ((await buttons.count()) === 0) {
    await page.waitForTimeout(250);
    continue;
  }
  if (!sawUndoWork && (await undoEnabled())) sawUndoWork = true;

  if ((await chapter()) >= 2) {
    // First human decision of chapter 2: the deal was a reveal.
    verdict = !(await undoEnabled());
    break;
  }

  // Rush the round along: prefer ending/passing.
  const n = await buttons.count();
  let clicked = false;
  for (let b = 0; b < n; b++) {
    if (/End turn|Pass/i.test(await buttons.nth(b).innerText())) {
      await buttons.nth(b).click();
      clicked = true;
      break;
    }
  }
  if (!clicked) await buttons.first().click();
  await page.waitForTimeout(200);
}

console.log('undo worked on plain decisions before the deal:', sawUndoWork);
console.log('undo dead at the first decision of chapter 2:', verdict);
console.log('errors:', errors.length ? errors : 'none');
await browser.close();
process.exit(sawUndoWork && verdict === true && errors.length === 0 ? 0 : 1);
