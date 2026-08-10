/** Watch bots play until a battle panel shows real rolled dice, and shoot it. */
import { appUrl, launchChromium } from './browser.mjs';

const shot = process.argv[2] ?? '/tmp/dice-live.png';
const browser = await launchChromium();
const page = await browser.newPage({ viewport: { width: 1500, height: 1150 } });
const errors = [];
page.on('pageerror', (e) => errors.push(e.message));
await page.goto(appUrl, { waitUntil: 'networkidle' });
await page.click('nav .tab >> nth=1'); // Watch
await page.waitForTimeout(800);

for (let i = 0; i < 400; i++) {
  if ((await page.locator('.battle-panel .die').count()) > 0) {
    const dice = await page.locator('.battle-panel .die').count();
    const head = await page.locator('.battle-head').innerText();
    console.log(`battle panel live: ${dice} dice — ${head}`);
    await page.screenshot({ path: shot, clip: { x: 950, y: 60, width: 540, height: 420 } });
    // The roll should also be in the log.
    const logged = await page.locator('.log li', { hasText: 'rolls' }).count();
    console.log('roll lines in log:', logged);
    console.log('errors:', errors.length ? errors : 'none');
    await browser.close();
    process.exit(0);
  }
  await page.waitForTimeout(250);
}
console.log('no battle within the wait; errors:', errors);
await browser.close();
process.exit(1);
