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

Leaders & Lore and the Blighted Reach campaign are out of scope. **All 31 Court
cards work as printed** — every Guild and Vox ability is dispatched, and
`POWER_STATUS` in `court.ts` plus a test that asserts nothing is left over keep
that claim honest.

One thing inside the base game is still reconstructed, because the data lives on
the physical components rather than in the rulebook:

- **Setup cards, and one detail of the map.** The map is transcribed from the
  printed board — all 18 planet types and building-slot counts, cross-checked
  against the type distribution (Material 4, Fuel 4, Weapon 4, Relic 3,
  Psionic 3) — on top of a structurally faithful graph (6 clusters, 1 gate + 3
  planets, ring adjacency, out-of-play clusters and path markers). What remains
  reconstructed is which intra-cluster planet pair a thick border splits, and
  setups are generated rather than drawn from the 12 printed cards.

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

Modest batches, and the setups are still generated rather than the printed 12, so
these describe *this engine's* Arcs.

Two players:

| matchup | games | win % | mean Power |
|---|---|---|---|
| `random+` vs `random` | 40 | 52.5 / 47.5 | 4.3 / 6.3 |
| `greedy` vs `random+` | 40 | **100.0** / 0.0 | 43.3 / 1.5 |
| `greedy` vs `mc` | 40 | 52.5 / 47.5 | 23.9 / 23.4 |
| `greedy` vs `mcts` | 60 | 28.3 / **71.7** | 21.0 / 25.5 |

Three players:

| field | games | win % |
|---|---|---|
| `greedy`, `mc`, `random` | 60 | **56.7** / 43.3 / 0.0 |

**`mcts` beats `greedy`, and that is new.** This matchup has been measured five
times as the rules got more accurate, and the first four readings — 50/50, 35/65,
55/45, 41.7/58.3 — all sat inside a ±12.6 interval, which this project reported as
"not separated" each time rather than picking the leader. The fifth is
71.7 ± 11.4, an interval of [60.3, 83.1] that excludes 50%. `mcts` also passes
`greedy` on mean Power for the first time, which had been the one stable
difference between them.

The likely cause is that a complete Court is the part of Arcs that rewards
lookahead: card abilities are conditional and sequenced, and a one-step evaluation
prices a setup move at nothing. That is a hypothesis, and the README says so —
testing it needs an abilities-off switch the engine does not have.

**Component data moves the ladder more than agent design does.** Three of those
five readings moved on *data*, not code. Transcribing the map alone swung `greedy`
vs `mc` by 22.5 points, larger than any change to an agent has produced. That is
the main methodological result here: in a game this component-heavy, an engine
built on plausible-looking reconstructed data produces a confident and wrong
strategy ranking, and the error is invisible from inside the numbers.

**Finishing the Court exposed a hole in the bots, not the engine.** Instrumenting
40 games shows three abilities offered constantly and never once taken — Farseers'
Prelude (998 offers), attaching a Union (119), Execute (15). One cause: `eval.ts`
has no term for hand quality, so trading cards for better cards evaluates as
zero. That is now the highest-value thing to add to the evaluation.

See [docs/FINDINGS.md](docs/FINDINGS.md) for the evidence and the negative
results.

## Layout

```
docs/RULES.md          the rulebook as engine ground truth, with page cites
docs/DATA-GAPS.md      component data the rulebook omits, and how to correct it
docs/FINDINGS.md       results, negative results, and methodology traps
src/engine/            pure engine, no dependencies
  types.ts             core types; the decision-process contract
  map.ts               the 6-cluster ring, the transcribed planets, adjacency
  cards.ts             the 28-card action deck
  court.ts             the 31 Guild and Vox cards, as data
  powers.ts            card ability dispatch: Preludes, new actions, passives, Vox
  dice.ts              battle die faces
  ambitions.ts         markers, counting, end-of-chapter scoring
  playerBoard.ts       resource slots, city rewards, raid costs
  board.ts             control, adjacency, the Cartel supply, state cloning
  setup.ts             variants and the opening position
  game.ts              the state machine
  observe.ts           the imperfect-information boundary
src/agents/            bots, the evaluation function, rollout plumbing
src/sim/               seeded runner, tournament stats, CLI
src/ui/                React app (Play / Watch / Simulate)
tests/                 vitest suite
```

## Tests

167 tests, in five groups:

- `components.test.ts` — the map graph and its planet distribution, deck
  composition, pip and ambition tables, die-face distributions, marker values.
- `rules.test.ts` — setup, lead/follow legality, initiative and seizing,
  declaring ambitions, every standard action, Prelude resources, battle
  resolution, ambition scoring including every tie case, chapter and game end.
- `powers.test.ts` — every Court card ability: the Loyal cards, both Cartels,
  the Unions, the new actions, Skirmishers' reroll, Farseers' peek and its
  information boundary, Galactic Bards, and all six Vox cards. Also asserts
  `UNIMPLEMENTED_POWERS` is empty, so the claim above cannot go stale.
- `engine.test.ts` — the decision-process contract: a legal action always
  exists, games terminate under adversarial policies, `applyAction` is pure,
  `cloneState` is deep, runs are reproducible, ships are conserved, and the
  observation boundary holds. Includes a stress game where every player holds
  the entire Court, which drives the rare ability paths thousands of times and
  asserts that the hard-to-reach ones actually ran.
- `agents.test.ts` — every registered agent finishes a game and never plays an
  action that was not offered.
