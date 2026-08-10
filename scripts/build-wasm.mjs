/**
 * Build the Rust engine into `src/ui/wasm/`, which is what the UI runs on.
 *
 * The generated package is a build artifact (gitignored), so this runs before
 * `dev`, `build` and `test`. It is a no-op when the artifact is already newer
 * than every Rust source, which keeps `npm test` fast.
 *
 * Needs `wasm-pack` and the `wasm32-unknown-unknown` target:
 *   rustup target add wasm32-unknown-unknown
 *   curl https://rustwasm.github.io/wasm-pack/installer/init.sh -sSf | sh
 */
import { execFileSync } from 'node:child_process';
import { readdirSync, statSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = dirname(dirname(fileURLToPath(import.meta.url)));
const crates = join(root, 'rust', 'crates');
const out = join(root, 'src', 'ui', 'wasm');
const artifact = join(out, 'arcs_wasm_bg.wasm');

/** Newest mtime under a directory, ignoring build output. */
function newest(dir) {
  let latest = 0;
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    if (entry.name === 'target' || entry.name === 'node_modules') continue;
    const path = join(dir, entry.name);
    latest = Math.max(latest, entry.isDirectory() ? newest(path) : statSync(path).mtimeMs);
  }
  return latest;
}

function mtime(path) {
  try {
    return statSync(path).mtimeMs;
  } catch {
    return 0;
  }
}

const force = process.argv.includes('--force');
if (!force && mtime(artifact) > newest(crates)) {
  console.log('wasm: up to date');
  process.exit(0);
}

console.log('wasm: building src/ui/wasm from rust/crates/arcs-wasm …');
try {
  execFileSync(
    'wasm-pack',
    ['build', 'crates/arcs-wasm', '--target', 'web', '--out-dir', '../../../src/ui/wasm', '--release'],
    { cwd: join(root, 'rust'), stdio: 'inherit' },
  );
} catch (e) {
  console.error(
    '\nwasm build failed. The UI runs on the Rust engine, so it needs:\n' +
      '  rustup target add wasm32-unknown-unknown\n' +
      '  curl https://rustwasm.github.io/wasm-pack/installer/init.sh -sSf | sh\n',
  );
  throw e;
}
console.log(`wasm: ${(statSync(artifact).size / 1024).toFixed(0)} KB`);
