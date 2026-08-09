/** Render the battle panel off a doctored mid-battle state, all die types out. */
import { writeFileSync, readFileSync } from 'node:fs';
import { chromium } from 'playwright';

const { renderToStaticMarkup } = await import('react-dom/server');
const { makeVariant, mulberry32, newGame } = await import('../src/engine/index.ts');
const { BattlePanel } = await import('../src/ui/components/Dice.tsx');
const React = (await import('react')).default;

const v = makeVariant(3, 0);
const s = newGame(v, mulberry32(4), 0);
s.battle = {
  // One of every face, so the whole vocabulary is on show.
  rolled: { assault: [0, 1, 2, 3, 5], skirmish: [0, 3], raid: [0, 1, 2, 3, 5] },
  system: 9,
  attacker: 0,
  defender: 1,
  dice: { assault: 5, skirmish: 2, raid: 5 },
  selfHits: 5,
  intercept: 3,
  hits: 6,
  buildingHits: 3,
  keys: 4,
  interceptResolved: true,
  skirmishBlanks: 1,
  pendingReroll: 0,
  rerollDone: false,
};

const html = renderToStaticMarkup(React.createElement(BattlePanel, { state: s, variant: v }));
const css = readFileSync(new URL('../src/ui/styles.css', import.meta.url), 'utf8');
writeFileSync('/tmp/dice.html', `<style>body{background:#0d1017;margin:16px;width:420px}${css}</style>${html}`);

const browser = await chromium.launch({ executablePath: '/opt/pw-browsers/chromium' });
const page = await browser.newPage({ viewport: { width: 460, height: 420 } });
await page.goto('file:///tmp/dice.html');
await page.screenshot({ path: process.argv[2] ?? '/tmp/dice.png' });
await browser.close();
console.log('ok');
