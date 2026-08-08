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
  reconstructed is which intra-cluster planet pair a thick border splits.
  Openings come from a **reconstructed setup deck** — 4 cards per player count,
  as the box has — chosen by scoring 20,000 legal layouts for balance rather
  than invented by hand. Every stated setup rule is obeyed and asserted per
  card; the printed 12 remain unknown.

Every such value is isolated, marked `// DATA-GAP:` in code, and catalogued in
[docs/DATA-GAPS.md](docs/DATA-GAPS.md) with the reconstruction used and how to
correct it. None of them affect the shape of the engine.

## The interface

The UI renders the real components rather than abstractions of them.

- **The board** is the printed wheel: a dark void carrying the wordmark, a ring
  of six numbered gates around it (1 at the top, 2–6 clockwise), and six tinted
  wedges holding their clusters' planets. Each planet shows its **building slots
  as outlined triangles** — the printed notation — filled in with the owner's
  colour once built, and carries its resource badge. Geometry is derived from
  the engine's graph, so replacing the map data moves the board with it.
- **Action cards** carry the suit as ink: the numeral and its pips top-left, the
  suit and its actions set vertically up the spine, art to the right, and the
  ambition on a tab at the top. When an ambition is declared, the **zero marker**
  covers the number exactly as the cardboard one does.
- **Court cards** come in the two printed forms — a Guild card with its
  raid-cost keys on a banner, its suit rosette, name plate and rules text; a Vox
  card with the title at the top and no rosette — both footed `BC | GUILD | nn`.

The artwork is **original**: these are SVG and CSS renderings of the printed
*layout*, palette and symbols, not scans. Leder Games' illustrations are their
copyright and this repository is public, so the card art and planet art are
stylised stand-ins. Suit colours are sampled from the rulebook where a sample
was available and flagged in `Glyphs.tsx` where they are matched by eye.

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
observer's own, while keeping hand *sizes*. `determinize` deals the unseen cards
back at random consistently with everything the observer has seen. The simulator
hands agents an `Observation`, so a bot cannot cheat by construction.

"Consistently" is load-bearing and was once wrong: cards played face up in
earlier rounds of a chapter were forgotten, so 65% of sampled worlds contained a
card the observer had personally watched being played. `GameState.revealed` is
the public memory that fixes it, and a test now asserts no sampled world contains
a discarded card.

Weighting the deal by what the follow history implies is implemented
(`src/engine/belief.ts`) and **off**, because it measured as no better than
uniform — Arcs has no follow-suit obligation, so declining to Surpass is a choice
rather than a confession. `tools/belief-eval.ts` is the harness that says so.

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
  and raid faces from published probabilities. The inference matched every quoted
  statistic and still had the wrong symbols sharing faces — the joint
  distribution was wrong where every marginal was right. (It was credited with
  moving the ladder too, but that was measured on the harness described under
  Results, so the size of the effect is unknown.)
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
| `--setup` | 0 | seeds the opening: which setup card, and who takes which position |
| `--free-setup` | off | invent a fresh legal opening per deal instead of drawing a card |
| `--opts` | — | JSON passed to every agent |
| `--no-rotate` | off | keep seats fixed instead of permuting |
| `--unpaired` | off | give every game its own deal (noisy and biased; for contrast only) |
| `--verbose` | off | one game with a turn log |

Output gives win rate with a confidence interval, mean Power, mean finishing
rank, per-ambition counts at game end, a Power histogram, and a **paired
head-to-head** verdict for the first two agents that says in as many words
whether the interval excludes zero.

**Deals are paired and seats are permuted**, and both are needed. Permuting
alone leaves the agents' cyclic order fixed, and in a lead-and-follow game
sitting immediately after a weak player is worth real points — two *identical*
greedy agents posted 78% / 22% under rotation. Pairing alone would leave seat
advantage in. Together they make the deal and the seat controlled variables:
identical agents come out exactly 50/50, with zero variance, and a test asserts
it.

## Results

All from the **paired** harness playing the **setup-card deck** — draw one of the
four cards for your player count, take positions on it at random, every deal
played from every seating. Numbers published before these fixes are not
comparable; see below.

Two players:

| matchup | games | deals | win % | paired difference | separated |
|---|---|---|---|---|---|
| `greedy` vs `random+` | 120 | 60 | **100.0** / 0.0 | +100.0 ±0.0 | yes |
| `mcts` vs `greedy` | 120 | 60 | **71.7** / 28.3 | +43.3 ±17.0 | yes |
| `random+` vs `random` | 2000 | 1000 | **60.5** / 39.6 | +20.9 ±4.2 | yes |
| `greedy` vs `mc` | 120 | 60 | 59.2 / 40.8 | +18.3 ±18.3 | **no** |

Three players:

| field | games | deals | win % |
|---|---|---|---|
| `greedy`, `mc`, `random` | 180 | 30 | **65.0** / 35.0 / 0.0 |

`mcts` > `greedy` > `mc` > `random+` > `random`, with `greedy` vs `mc` the one
rung not settled head-to-head — though `greedy` takes the 3-player field.

**The opening is a variable.** Setups follow the game: shuffle the four setup
cards for your player count, draw one, and take positions on it — with *which*
position each player gets, and turn order, both randomised, since the card
decides neither. The same matchup across three ways of choosing the opening,
same agents throughout:

| openings | `greedy` vs `mc` | separated |
|---|---|---|
| 6 fixed rotations (originally) | +31.7 ±15.1 | yes |
| 4 setup cards (as played) | +18.3 ±18.3 | no |
| ~3000 free draws (`--free-setup`) | +13.3 ±17.1 | no |

Some of that original edge was an edge *on those six boards*. The 3-player field
held its separation throughout, which is the recurring pattern here: the
multiplayer result is the durable one.

**The measurement was broken, and finding that out was worth more than any result
it produced.** The batch runner advanced the seed *and* the setup index on every
game while cycling seatings through the `n!` permutations. At two players that
aliases seating to setup parity exactly — one agent got every even setup, the
other every odd one — so the permutations cancelled nothing. Two **identical**
greedy agents, eight replications:

| scheme | mean win-share gap | sd |
|---|---|---|
| paired (now) | **0.00** | 0.00 |
| unpaired (before) | **−13.75** | 12.46 |

An agent losing to a copy of itself by 14 points is larger than most effects this
project reported. Fixing it forced two corrections: `random+` really does beat
`random` (called "no improvement" four times), and `greedy` beats `mc` in both
player counts, undoing a "flip" previously credited to the map transcription. The
`mcts` result survived, at 65.0 rather than 71.7.

Worth noting what pairing did *not* do: the spread is unchanged (sd 9.49 either
way). Common random numbers only reduce variance when the agents' play stays
correlated, and here it diverges within a few decisions. The win was removing a
confound, not tightening an interval.

**A second measurement bug, in the search itself.** `determinize()` forgot cards
played face up in earlier rounds of the chapter, so **65% of sampled worlds
contained a card the observer had watched being played** — 1.56 apiece. Search
agents were spending budget on impossible worlds. Now zero.

**Finishing the Court exposed a hole in the bots, not the engine.** Three
abilities are offered constantly and never once taken — Farseers' Prelude (998
offers in 40 games), attaching a Union (119), Execute (15). `eval.ts` has no term
for hand quality, so trading cards for better cards evaluates as zero.

See [docs/FINDINGS.md](docs/FINDINGS.md) for the evidence, the negative results,
and a belief model that measured as worthless and why.

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
  belief.ts            hand inference from the public record (measured null)
src/agents/            bots, the evaluation function, rollout plumbing
src/sim/               paired seeded runner, tournament stats, CLI
src/ui/                React app (Play / Watch / Simulate)
  components/Board.tsx   the wheel: void, gate ring, cluster wedges, planets
  components/Cards.tsx   action / Guild / Vox card faces
  components/Glyphs.tsx  resource, key and pip symbols; the palette
tools/                 calibration and offline-metric scripts
tests/                 vitest suite
```

## Tests

174 tests, in five groups:

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
