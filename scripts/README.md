# scripts

- `build-wasm.mjs` — compiles `rust/crates/arcs-wasm` into `src/ui/wasm/`,
  which is the engine the UI runs on. Wired into `predev`/`prebuild`/`pretest`
  and a no-op when the artifact is newer than every Rust source; `--force`
  rebuilds anyway. Needs `rustup target add wasm32-unknown-unknown` and
  [`wasm-pack`](https://rustwasm.github.io/wasm-pack/installer/).
- `browser.mjs` — where the other scripts get Chromium (`PW_CHROMIUM`, else the
  sandbox's `/opt/pw-browsers/chromium`, else Playwright's own download) and
  the dev-server URL (`ARCS_URL`, default `http://localhost:5173/`).
- `screenshot.mjs` — launch the dev server (`npm run dev`), then
  `node scripts/screenshot.mjs out.png` to capture the UI. Used to check the
  board renders after map or layout changes.
- `watch-shot.mjs` — the same, but sets every seat to a bot and lets the game
  run before shooting: `node scripts/watch-shot.mjs out.png 30000`. For states
  a fresh deal never shows.
- `board-preview.mjs` — renders the board alone, off a doctored state, without
  the dev server: `npx tsx scripts/board-preview.mjs out.png`. Reaches states
  that are rare in play — the file currently damages ships on every planet, to
  check fresh and damaged read differently.
- `undo-check.mjs` / `pip-check.mjs` — drive the play tab with a bot-vs-human
  game: the first takes an action, undoes it and checks the state rewinds
  exactly; the second clicks until a spent pip renders as a filled circle.
- `undo-barrier-check.mjs` — checks undo is dead at the first decision after
  the chapter deal, the same chance-node path as a battle roll.
- `dice-preview.mjs` — renders the battle panel off a doctored mid-battle
  state with one of every die face: `npx tsx scripts/dice-preview.mjs out.png`.
- `dice-live-check.mjs` — watches bots play until a battle panel shows real
  rolled dice, then screenshots it and checks the roll reached the log.
