# scripts

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
