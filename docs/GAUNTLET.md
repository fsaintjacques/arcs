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
