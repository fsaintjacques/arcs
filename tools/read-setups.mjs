/**
 * Redraw the map's sector names over a printed setup card, so the transcription
 * in `SETUP_DECK` can be re-checked by eye.
 *
 *   node tools/read-setups.mjs card.jpg annotated.png
 *
 * The card draws the map schematically: 18 planet slices in a ring around 6
 * gate sectors, with a position label ("2B", "1C") printed in each system a
 * player starts in. Reading a card is therefore geometry, not OCR — every
 * sector's angular span is known, so a label's angle names its system.
 *
 * The numbers below were measured from the pixel-wise median of all 12 cards,
 * which erases their labels and out-of-play shading and leaves the bare border
 * drawing (see docs/DATA-GAPS.md). Cards are 1004x719 and share a frame; the
 * ring is elliptical, which is why the 18 borders are not 20 degrees apart.
 *
 * Requires ImageMagick's `convert` on PATH.
 */
import { execFileSync } from 'node:child_process';

const CX = 495;
const CY = 375;

/** Every radial border, in degrees clockwise from straight up. */
const BORDERS = [
  1.3, 29.8, 51.8, 69.3, 84.0, 97.3, 111.8, 128.8, 147.3,
  178.3, 206.8, 231.3, 247.5, 262.5, 276.3, 292.3, 310.3, 330.5,
];
/** Every third one is a cluster boundary; 330.5 is the 6|1 seam. */
const CLUSTER_EDGE = [51.8, 97.3, 147.3, 231.3, 276.3, 330.5];

const at = (deg, r) => {
  const a = (deg * Math.PI) / 180;
  return [CX + r * Math.sin(a), CY - r * Math.cos(a)];
};
const mid = (a, b) => (b > a ? (a + b) / 2 : ((a + b + 360) / 2) % 360);

const labels = [];
const start = BORDERS.indexOf(330.5);
for (let k = 0; k < 18; k++) {
  const [x, y] = at(mid(BORDERS[(start + k) % 18], BORDERS[(start + k + 1) % 18]), 232);
  labels.push([x, y, `${Math.floor(k / 3) + 1}.${(k % 3) + 1}`]);
}
for (let c = 0; c < 6; c++) {
  const [x, y] = at(mid(CLUSTER_EDGE[(c + 5) % 6], CLUSTER_EDGE[c]), 120);
  labels.push([x, y, `G${c + 1}`]);
}

const [src, out = 'annotated.png'] = process.argv.slice(2);
if (!src) {
  console.error('usage: node tools/read-setups.mjs <card.jpg> [out.png]');
  process.exit(1);
}

execFileSync('convert', [
  src, '-resize', '200%',
  '-fill', 'none', '-stroke', '#ff2d55', '-strokewidth', '2',
  ...BORDERS.flatMap((b) => {
    const [x0, y0] = at(b, 150).map((n) => n * 2);
    const [x1, y1] = at(b, 300).map((n) => n * 2);
    return ['-draw', `line ${Math.round(x0)},${Math.round(y0)} ${Math.round(x1)},${Math.round(y1)}`];
  }),
  '-stroke', 'none', '-fill', '#0066ff', '-pointsize', '30', '-weight', 'bold',
  ...labels.flatMap(([x, y, text]) => [
    '-draw', `text ${Math.round(x * 2 - 32)},${Math.round(y * 2)} '${text}'`,
  ]),
  out,
]);
console.log('wrote', out);
