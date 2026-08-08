# Arcs Lab

A browser and command-line implementation of the **Arcs** base game (Cole
Wehrle, Leder Games), built as a strategy laboratory: a dependency-free
TypeScript engine, pluggable bots, headless tournaments, and a web UI to play
against the bots, watch them play each other, and run batches.

```bash
npm install
npm run dev     # web UI at localhost:5173 — Play / Watch / Simulate
npm test        # rules and engine-contract suite
npm run sim -- --agents greedy,greedy,random --games 200
```

Arcs is a 2–4 player game of **imperfect information**, which is what makes it
an interesting lab: bots are handed a legal *observation* rather than the game
state, and search agents re-sample consistent worlds from it.

> Unofficial fan-made implementation for personal strategy research. Not
> affiliated with or endorsed by Leder Games. Game design by Cole Wehrle.

## Scope

The **base game**: 2–4 players, up to 5 chapters, the lead-and-follow action
deck, ambitions and Power, the Court, resources, building, movement with
Catapult, battle with all three dice, raiding, and Outrage.

Leaders & Lore and the Blighted Reach campaign are out of scope. Two things
inside the base game are not fully modelled, both because the data lives on the
physical components rather than in the rulebook:

- **Guild and Vox card abilities.** All 31 cards are present with their real
  names, suits, raid costs and printed text, so influence, secure, raid, Outrage
  discards and the Tycoon / Keeper / Empath counts are exact — but the printed
  *abilities* are not dispatched yet. `IMPLEMENTED_POWERS` in `court.ts` tracks
  what is wired up, and a test keeps that list honest. Until it is complete,
  bots under-value the Court.
- **The printed map and setup cards.** The map is structurally faithful
  (6 clusters, 1 gate + 3 planets, ring adjacency, out-of-play clusters and path
  markers) but the planet-type layout is invented, and setups are generated
  rather than drawn from the 12 printed cards.

Every such value is isolated, marked `// DATA-GAP:` in code, and catalogued in
[docs/DATA-GAPS.md](docs/DATA-GAPS.md) with the reconstruction used and how to
correct it. None of them affect the shape of the engine.

## The engine as a library

The game is modelled as an explicit sequential decision process, so any
algorithm can drive it:

```ts
import { makeVariant, newGame, getPending, applyAction, resolveChance, mulberry32, standings } from './src/engine';

const variant = makeVariant(3);
const rng = mulberry32(42);
let s = newGame(variant, rng);

for (;;) {
  const node = getPending(s, variant);
  if (node.kind === 'over') break;
  if (node.kind === 'chance') { s = resolveChance(s, variant, rng); continue; }
  s = applyAction(s, variant, node.actions[0]);   // your policy here
}

console.log(standings(s));
```

- **Decision nodes** name the player to move and enumerate every legal `Action`
  — cards to lead, follow modes, ambition declarations, seizes, Prelude
  resource spends, each standard action fully specified, and battle dice pools.
- **Chance nodes** are the chapter deal and the battle dice roll, resolved by an
  injectable seeded RNG, so games replay exactly and search can sample.
- Cascades that the rules resolve inside one action — a battle's hits, a
  catapult's onward legs — surface as ordinary decision nodes.
- `applyActionMut` / `resolveChanceMut` / `cloneState` are the allocation-free
  variants for hot loops.

### Imperfect information

Hands and deck order are hidden (rulebook p22). Two functions draw the line:

```ts
const obs = observe(state, variant, player);   // legal view: rival hands blanked
const world = determinize(obs, variant, rng);  // a consistent full state to search
```

`observe` blanks rival hands, every deck, and face-down plays that are not the
observer's own, while keeping hand *sizes*. `determinize` deals the unseen
cards back at random consistently with everything the observer has seen. The
simulator hands agents an `Observation`, so a bot cannot cheat by construction.

## Writing an agent

An agent is one function:

```ts
interface Agent {
  name: string;
  choose(obs: Observation, actions: Action[], ctx: { variant; rng; player }): Action;
}
```

Register it in `src/agents/index.ts` and it appears in the CLI and both UI
modes. Bundled agents:

| agent | idea |
|---|---|
| `random` | uniform over legal actions — the floor |
| `random+` | random but never idles a turn away; the rollout policy |
| `greedy` | one-step lookahead over the heuristic, with cascades settled |
| `greedy-flat` | the same without cascade settling, to price the settling |
| `mc` | flat Monte-Carlo over sampled worlds |
| `mcts` | determinized ISMCTS with max^n backup |
| `mcts-fast` | the same, cheap enough for large batches |

The evaluation function in `src/agents/eval.ts` is the main experimentation
surface: banked Power, what the declared ambition boxes are currently worth,
and the latent value of an economy and fleet not yet cashed in. All weights are
parameters.

Three details that turned out to matter more than the search itself:

- **Cascade settling.** A one-step eval scores `battle` *before* the dice exist,
  so it never fights: in a 2-player batch, battle was offered 163 times and
  taken 0. Playing the cascade out before scoring took that to 104 of 164 —
  worth more than any weight tuning so far.
- **Exact die faces, not matching odds.** The first version inferred the assault
  and raid faces from published probabilities. The inference matched every
  quoted statistic and still had the wrong symbols sharing faces, which flipped
  `mcts` vs `greedy` from an even split to 65/35.
- **max^n backup, not minimax.** With 3–4 players, backing up a single scalar
  and assuming the opponents minimise your score models a coalition that is not
  in the game. Each node carries a value per seat and selection maximises the
  value of the seat to move there.

## Simulating from the command line

```bash
npm run sim -- --agents greedy,greedy,random --games 200 --seed 11
npm run sim -- --agents mcts,greedy --games 50 --opts '{"iterations":800}'
npm run sim -- --agents greedy,greedy --games 1 --verbose      # turn log
npm run sim -- --help
```

| flag | default | meaning |
|---|---|---|
| `--agents` | `greedy,greedy,greedy` | one per seat; 2–4 of them sets the player count |
| `--games` | 100 | batch size |
| `--seed` | 1 | games are fully reproducible from this |
| `--setup` | 0 | starting setup; each game rotates on from here |
| `--opts` | — | JSON passed to every agent |
| `--no-rotate` | off | keep seats fixed instead of permuting |
| `--verbose` | off | one game with a turn log |

Output gives win rate with a confidence interval, mean Power, mean finishing
rank, per-ambition counts at game end, and a Power histogram.

**Seats are permuted, not rotated.** Rotating seats leaves the agents' cyclic
order fixed, and in a lead-and-follow game sitting immediately after a weak
player is worth real points: two *identical* greedy agents posted 78% / 22%
under rotation and 50% / 50% under permutation. That bug would have made every
comparison in this README wrong.

## Results

Modest batches, and the caveats above apply — Guild card abilities are not
dispatched yet and the map is not the printed one, so these describe *this
engine's* Arcs.

Two players:

| matchup | games | win % | mean Power |
|---|---|---|---|
| `random+` vs `random` | 40 | 52.5 / 47.5 | 3.9 / 4.1 |
| `greedy` vs `random+` | 40 | **100.0** / 0.0 | 44.6 / 2.4 |
| `greedy` vs `mc` | 40 | **77.5** / 22.5 | 32.9 / 16.2 |
| `mcts` vs `greedy` | 60 | **65.0** / 35.0 | 23.2 / 24.3 |

Three and four players:

| field | games | win % |
|---|---|---|
| `greedy`, `mc`, `random` | 60 | **63.3** / 36.7 / 0.0 |
| `greedy`, `greedy`, `mc`, `random` | 48 | **50.0** / 35.4 / 14.6 / 0.0 |

Three results are worth more than the ordering:

**Flat Monte-Carlo loses to one-step greedy**, 22.5/77.5, despite sampling
strictly more information. `mc` has no tree, so it cannot see its own follow-up
pips — and an Arcs turn is a *sequence* of 1–4 dependent actions (build a
starport, then build a ship at it; move in, then battle). Valuing the first
action of that sequence against a random continuation prices the setup at
nothing.

**MCTS beats greedy 65/35** while scoring slightly *less* Power (23.2 vs 24.3).
That is what optimising the right objective looks like: rollouts are valued on
final standing, not on Power, so the search prefers winning by 1 over losing by
5 with a bigger number on the track.

**`random+` is not actually better than `random`** — 52.5/47.5 is inside the
±15.5 interval. "Never waste a turn" sounds like an improvement and is not one
at this level, which is a useful reminder that a plausible heuristic is a
hypothesis, not a fact.

All four numbers moved when the battle dice were corrected from the aid
booklet; the `mcts` row moved most, from an even split to a clear win. See
[docs/FINDINGS.md](docs/FINDINGS.md).

## Layout

```
docs/RULES.md          the rulebook as engine ground truth, with page cites
docs/DATA-GAPS.md      component data the rulebook omits, and how to correct it
docs/FINDINGS.md       results, negative results, and methodology traps
src/engine/            pure engine, no dependencies
  types.ts             core types; the decision-process contract
  map.ts               the 6-cluster ring, adjacency, out-of-play clusters
  cards.ts             the 28-card action deck
  court.ts             Guild and Vox cards
  dice.ts              battle die faces
  ambitions.ts         markers, counting, end-of-chapter scoring
  playerBoard.ts       resource slots, city rewards, raid costs
  board.ts             control, adjacency and state cloning
  setup.ts             variants and the opening position
  game.ts              the state machine
  observe.ts           the imperfect-information boundary
src/agents/            bots, the evaluation function, rollout plumbing
src/sim/               seeded runner, tournament stats, CLI
src/ui/                React app (Play / Watch / Simulate)
tests/                 vitest suite
```

## Tests

104 tests, in four groups:

- `components.test.ts` — the map graph, deck composition, pip and ambition
  tables, die-face distributions, marker values.
- `rules.test.ts` — setup, lead/follow legality, initiative and seizing,
  declaring ambitions, every standard action, Prelude resources, battle
  resolution, ambition scoring including every tie case, chapter and game end.
- `engine.test.ts` — the decision-process contract: a legal action always
  exists, games terminate under adversarial policies, `applyAction` is pure,
  `cloneState` is deep, runs are reproducible, ships are conserved, and the
  observation boundary holds.
- `agents.test.ts` — every registered agent finishes a game and never plays an
  action that was not offered.
