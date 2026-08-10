# The strength gauntlet

The gauntlet is the only way a bot earns a strength claim in this repo. It
exists because [FINDINGS.md](FINDINGS.md) shows the two ways such claims go
wrong here: unpaired batches measure the shuffle, and slow bots starve the
batch sizes that would have caught it. Every candidate therefore runs the same
protocol, and every result — including failures — lands in the ledger below.

## Protocol

- **Table**: 3 players — the candidate and two copies of one frozen anchor.
- **Harness**: the standard paired runner (`simulate`): every deal is played
  from all 6 seatings, and the paired comparison reads within-block win-share
  differences (`pairedStats`). Setup cards from the printed deck.
- **Sample**: ≥240 games per anchor (40 deals × 6 seatings) for a promotion
  claim. Smaller runs are fine for iteration but don't enter the ledger.
- **Budget**: mean thinking time ≤ **30 ms/decision** at the measurement
  preset. Speed is part of the pass rule — batch variance is the binding
  constraint on everything else in this lab, so a bot that cannot be measured
  cheaply is a worse instrument whatever its strength.
- **Pass rule**: separated positive diff vs the **newest** anchor, no
  separated regression vs any older anchor, budget respected.

Run it with:

```bash
npx tsx tools/gauntlet.ts --candidate <agent> --games 240 --seed 1
```

## Anchors

Anchors are frozen agents (`src/agents/anchors.ts`) — append-only, never
retuned. `makeAgent(name)` with no opts is the anchor's identity.

| anchor | frozen | what it is |
|---|---|---|
| `anchor-greedy-v0` | 2026-08-08 | `greedy` with the original hand-set weights |
| `anchor-mcts300-v0` | 2026-08-08 | `mcts` at 300 iterations, original weights — the ladder's top when the gauntlet was established |
| `anchor-mcts-c-v1` | 2026-08-09 | `mcts` at 300 iterations with informed candidate trimming and the M2 hand-set weights (`anchorWeightsV1`) |

A candidate that passes the gauntlet is frozen as the next anchor generation
before work on its successor begins.

## Ledger

Append-only. One row per (candidate, anchor) pair, produced by
`tools/gauntlet.ts`. Positive diff favours the candidate.

| date | candidate | anchor | games | diff ±95% | separated | ms/dec | notes |
|---|---|---|---|---|---|---|---|
| 2026-08-09 | greedy (M2 eval) | anchor-greedy-v0 | 240 | +33.8±10.8 | yes | 0.2 | seed 11 |
| 2026-08-09 | greedy (M2 eval) | anchor-greedy-v0 | 240 | +41.3±14.4 | yes | 0.2 | seed 42, replication |
| 2026-08-09 | mcts (M2 eval) | anchor-mcts300-v0 | 240 | +6.3±13.2 | no | 9.6 | seed 11; promoted on no-regression — see FINDINGS on rollout dilution |
| 2026-08-09 | greedy `battles:'exact'` | anchor-greedy-v0 | 240 | +27.5±12.6 | yes | 0.7 | seed 11; 6.3 behind the sampler on the same deals — not adopted, see FINDINGS |
| 2026-08-09 | greedy `battles:'exact'` | anchor-greedy-v0 | 240 | +27.5±14.0 | yes | 1.3 | seed 42; 13.8 behind the sampler — not adopted |
| 2026-08-09 | greedy-t1 (CEM run 1) | anchor-greedy-v0 | 240 | +45.0±13.9 | yes | 0.2 | seed 11 |
| 2026-08-09 | greedy-t1 | greedy | 960 | +18.1±6.5 | yes | 0.2 | seed 7, held-out |
| 2026-08-09 | greedy-t1 | greedy | 960 | +5.3±6.4 | no | 0.2 | seed 99, held-out |
| 2026-08-09 | mcts-t1 | mcts | 240 | -6.3±13.2 | no | 10.2 | seed 11 |
| 2026-08-09 | mcts-t1 | anchor-mcts300-v0 | 240 | +11.3±11.9 | no | 9.9 | seed 11 |
| 2026-08-09 | mcts-t1 | anchor-mcts300-v0 | 240 | +2.5±10.5 | no | 9.6 | seed 42; transfer gate FAILED — weights not promoted, no new anchor |
| 2026-08-09 | mcts-c | mcts | 240 | +15.0±12.7 | yes | 10.5 | seed 11; pure trim ablation — same budget, same weights |
| 2026-08-09 | mcts-c | anchor-mcts300-v0 | 240 | +22.5±11.6 | yes | 9.7 | seed 11; PASSED — frozen as anchor-mcts-c-v1 |
| 2026-08-09 | mcts-c | anchor-mcts300-v0 | 240 | +16.3±13.3 | yes | 11.4 | seed 42, replication |

## The Rust engine

The engine, the agents and this harness are being ported to Rust (`rust/`,
R-series milestones). The port keeps **statistical parity only** — a clean
SplitMix64 rather than a replication of `mulberry32` and the JS seed quirks —
so **seeds do not transfer**, and no Rust row can be compared game-for-game
with a row above. The ledger therefore restarts here. The anchor *weights* are
copied verbatim (`rust/crates/arcs-agents/src/anchors.rs`, asserted field for
field against a fixture printed from `src/agents/anchors.ts`), so the anchors
are the same bots, measured on fresh deals.

Run it with:

```bash
cd rust && cargo run --release -p arcs-cli -- gauntlet --candidate mcts-c --games 240 --seed 1
```

Protocol, sample size, budget and pass rule are unchanged — the Rust harness
ports them as invariants, including the two the ledger above exists to protect:
permuted (not rotated) seatings, and one deal per block of `n!` games. Two
notes specific to the Rust side:

- The Rust ladder carries a fourth anchor, **`anchor-mcts2-v2`** (the truncated
  PUCT search, frozen with R5). It has no TS ledger rows because `mcts2` landed
  after the TS ladder stopped being extended.
- `mcts2-play` must never be measured here. It carries a wall-clock
  per-decision budget, so it searches deeper on an idle machine than a busy
  one — an interactive preset, not a measurement preset.

**Dual-engine caution.** Until the UI migrates off the TypeScript engine, a
rules change has to land in both engines or be frozen on the TS side. A rules
divergence would not show up as a test failure; it would show up as a
calibration row drifting, which is exactly the kind of slow poisoning the
methodology traps in [FINDINGS.md](FINDINGS.md) are about.

### Calibration against the TypeScript ledger

The acceptance test for the harness port: reproduce TS ledger matchups in Rust
at equal or larger sample and check the Rust reading lands inside the TS row's
interval. Nothing was tuned to make a number match — the point of the exercise
is destroyed if anything was. Fourteen of sixteen comparisons land inside the
TypeScript interval; the discussion, including the one reading that does
**not**, is in
[FINDINGS.md](FINDINGS.md#the-rust-harness-reproduces-the-ledger-and-disagrees-about-one-thing).

### Ledger (Rust)

Append-only, and not comparable with the TS ledger above.

| date | candidate | anchor | games | diff ±95% | separated | ms/dec | notes |
|---|---|---|---|---|---|---|---|
| 2026-08-10 | greedy@f427a1a | anchor-greedy-v0 | 240 | +25.0±14.9 | yes | 0.02 | seed 11; calibration, TS row +33.8±10.8 |
| 2026-08-10 | greedy@f427a1a | anchor-greedy-v0 | 240 | +48.8±12.9 | yes | 0.02 | seed 42; calibration, TS row +41.3±14.4 |
| 2026-08-10 | greedy@f427a1a | anchor-greedy-v0 | 2400 | +37.6±4.2 | yes | 0.02 | seed 1; 10× sample, the sharp estimate |
| 2026-08-10 | greedy `battles:'exact'`@f427a1a | anchor-greedy-v0 | 240 | +30.0±13.5 | yes | 0.03 | seed 11; TS row +27.5±12.6 |
| 2026-08-10 | greedy `battles:'exact'`@f427a1a | anchor-greedy-v0 | 240 | +40.0±11.7 | yes | 0.05 | seed 42; TS row +27.5±14.0 |
| 2026-08-10 | greedy `battles:'exact'`@f427a1a | anchor-greedy-v0 | 2400 | +42.4±4.2 | yes | 0.02 | seed 1; 1–3 pts above the TS interval — see FINDINGS |
| 2026-08-10 | greedy `battles:'exact'`@f427a1a | greedy | 2400 | +1.8±3.9 | no | 0.02 | seed 1; head-to-head TS never ran — see FINDINGS |
| 2026-08-10 | mcts@f427a1a | anchor-mcts300-v0 | 240 | +18.8±13.9 | yes | 2.50 | seed 11; TS row +6.3±13.2 |
| 2026-08-10 | mcts@f427a1a | anchor-mcts300-v0 | 960 | +1.9±6.1 | no | 2.33 | seed 1; 4× sample |
| 2026-08-10 | mcts-c@f427a1a | mcts | 240 | +18.8±15.2 | yes | 2.57 | seed 11; TS row +15.0±12.7 |
| 2026-08-10 | mcts-c@f427a1a | mcts | 960 | +16.2±6.2 | yes | 2.47 | seed 1; 4× sample |
| 2026-08-10 | mcts-c@f427a1a | anchor-mcts300-v0 | 240 | +17.5±12.4 | yes | 2.58 | seed 11; TS row +22.5±11.6 |
| 2026-08-10 | mcts-c@f427a1a | anchor-mcts300-v0 | 240 | +27.5±13.6 | yes | 2.49 | seed 42; TS row +16.3±13.3 |
| 2026-08-10 | mcts-c@f427a1a | anchor-mcts300-v0 | 960 | +23.1±6.7 | yes | 2.55 | seed 1; 4× sample |

No candidate is promoted on these rows: they are calibration, run against the
anchors the TypeScript ledger used, not a claim about a new bot. `anchor-mcts2-v2`
is the newest anchor on the Rust ladder and nothing above has been measured
against it.

## Human validation

The gauntlet measures bots against bots. The final gate for the human-level
project also requires live games: ≥15 maintainer games against the interactive
preset of the current best bot (3p, one anchor in the third seat, seats
rotated by the maintainer). The bot must hold at least its fair share (1/3)
cumulatively, with no category-A blunders (missed lethal Power, ignored
declarable ambition lead, refusing all battles, dumping the hand into a
rival's Surpass) over the last 10 games. Protocol details land with the Play
UI game-record export (M10 of the plan).

| date | bot | seats | result | blunders | notes |
|---|---|---|---|---|---|
