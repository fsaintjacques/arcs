/** Run an all-bot game for a while, then screenshot — used to catch damaged ships. */
import { appUrl, launchChromium } from './browser.mjs';

const out = process.argv[2] ?? '/tmp/watch.png';
const waitMs = Number(process.argv[3] ?? 25000);
const browser = await launchChromium();
const page = await browser.newPage({ viewport: { width: 1500, height: 1150 } });
const errors = [];
page.on('console', (m) => { if (m.type() === 'error') errors.push(m.text()); });
page.on('pageerror', (e) => errors.push(`pageerror: ${e.message}`));

await page.goto(appUrl, { waitUntil: 'networkidle' });
await page.selectOption('.controls select >> nth=1', 'greedy');
await page.fill('.controls input >> nth=2', '10');
await page.click('button.primary');
await page.waitForTimeout(waitMs);

const damaged = await page.locator('.ship-damaged').count();
console.log('damaged ship stacks on board:', damaged);
console.log('errors:', errors.length ? errors.slice(0, 5) : 'none');

// Open the first player's Guild cards if any are held.
const guildButtons = page.locator('.chip-button');
const n = await guildButtons.count();
console.log('guild buttons:', n);
if (n > 0) await guildButtons.first().click();
await page.waitForTimeout(300);
console.log('guild cards shown:', await page.locator('.guild-cards .court-card').count());

await page.screenshot({ path: out, fullPage: true });
await browser.close();
