import { appUrl, launchChromium } from './browser.mjs';

const out = process.argv[2] ?? '/tmp/shot.png';
const browser = await launchChromium();
const page = await browser.newPage({ viewport: { width: 1500, height: 1150 } });
const errors = [];
page.on('console', (m) => { if (m.type() === 'error') errors.push(m.text()); });
page.on('pageerror', (e) => errors.push(`pageerror: ${e.message}`));

await page.goto(appUrl, { waitUntil: 'networkidle' });
await page.waitForTimeout(2500);
await page.screenshot({ path: out, fullPage: true });
console.log('systems drawn:', await page.locator('.system').count());
console.log('action buttons:', await page.locator('.action-list button').count());
console.log('status:', (await page.locator('.status-line').first().innerText()).replace(/\n/g, ' | '));
console.log('errors:', errors.length ? errors.slice(0, 5) : 'none');
await browser.close();
