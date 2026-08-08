/** Render the board with a doctored state, so damage is visible without playing. */
import { writeFileSync, readFileSync } from 'node:fs';
import { chromium } from 'playwright';

const { renderToStaticMarkup } = await import('react-dom/server');
const { makeVariant, mulberry32, newGame } = await import('../src/engine/index.ts');
const { Board } = await import('../src/ui/components/Board.tsx');
const React = (await import('react')).default;

const v = makeVariant(3, 0);
const s = newGame(v, mulberry32(4), 0);
for (let i = 0; i < s.systems.length; i++) {
  if (s.systems[i].outOfPlay || i % 4 === 0) continue;
  s.systems[i].damaged[i % 3] = (i % 3) + 1;
}
const svg = renderToStaticMarkup(React.createElement(Board, { state: s, variant: v }));
const css = readFileSync(new URL('../src/ui/styles.css', import.meta.url), 'utf8');
writeFileSync('/tmp/board.html', `<style>body{background:#0d1017;margin:0}${css}</style><div style="width:900px">${svg}</div>`);

const browser = await chromium.launch({ executablePath: '/opt/pw-browsers/chromium' });
const page = await browser.newPage({ viewport: { width: 940, height: 960 } });
await page.goto('file:///tmp/board.html');
await page.screenshot({ path: process.argv[2] ?? '/tmp/board.png' });
await browser.close();
console.log('ok');
