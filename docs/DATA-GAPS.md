# Component data not in the rulebook

The base rulebook (April 11 2024) documents every *rule* the engine needs, and
the official aid booklet supplies the battle dice, but several pieces of
*component data* live only on the physical components. Each one below is
reconstructed, isolated in a single data module, and marked in code with
`// DATA-GAP:` so it can be corrected with a data edit and no logic change.

Nothing here affects the shape of the engine — the decision process, the action
space, and the agent API are all independent of these values.

| # | Datum | Module | Confidence |
|---|---|---|---|
| 1 | Ambition marker reverse sides | `src/engine/ambitions.ts` | medium — one of three confirmed |
| 2 | Intra-cluster planet adjacency | `src/engine/map.ts` | medium — types and slots now read off the board; which pair a thick border splits is not |
| 3 | Setup cards | `src/engine/setup.ts` | low — a balance-scored deck of 4 per count, not the printed 12 |
| 4 | Player board economy | `src/engine/playerBoard.ts` | low |

**Closed:** battle die faces (official aid booklet p3 prints all six faces of
each die), the Court card data (names, suits, raid costs and verbatim text for
all 31 cards), the map's **planet types and building-slot counts** transcribed
from the printed board, and — as of this pass — **every Court card ability**,
all 31 now dispatched.

---

## 1. Ambition marker reverse sides

Blue (starting) sides are legible in the rulebook (p3): **5/3, 3/2, 2/0**. The
"2/0" marker flips to **4/2** (community-sourced). The other two reverses are
extrapolated on the same pattern (roughly doubled first place, second place a
little under half):

| Blue | Orange (engine) |
|---|---|
| 2/0 | 4/2 (sourced) |
| 3/2 | 6/4 (extrapolated) |
| 5/3 | 9/5 (extrapolated) |

This ordering is self-consistent with the flip rule ("flip the lowest-Power
marker that hasn't been flipped"), which flips 2/0 after chapter 1, 3/2 after
chapter 2 and 5/3 after chapter 3.

To correct: edit `AMBITION_MARKERS` in `ambitions.ts`.

## 2. Map layout

**Faithful**: 6 clusters, each 1 gate + 3 planets; gates form a ring; every gate
adjacent to its 3 planets and its 2 neighbouring gates; planets adjacent to
their gate; out-of-play clusters and path markers. And, since the transcription
below, **which planet type sits in which of the 18 slots and how many building
slots each planet has**.

**Still reconstructed**: which intra-cluster planet pair is separated by a thick
border. The engine models each cluster's planets as a path — planet 1–2 and 2–3
adjacent, 1–3 not — which satisfies "adjacent to one or both neighbouring
planets" (p6) but is a uniform assumption rather than a per-cluster reading.

### The transcription

Source: the **high-resolution base rulebook** (27 MB, as shipped in the
ArcsFates repo — not the 5 MB web copy) prints the full board on page 4.
Extract it with:

```bash
pdfimages -f 4 -l 4 -png "Arcs Base Rulebook.pdf" out    # out-003.png, 2016x1375
```

At 2.3× magnification the per-planet detail reads cleanly:

- **building slots** — count the outlined triangles on each planet (1 or 2);
- **planet type** — a filled badge beside each planet: gold coin-stack = Fuel,
  orange/magenta cube = Material, orange rocket = Weapon, blue oval face =
  Psionic, pale diamond = Relic;
- **gate numbering** — 1 at the top, then **2–6 clockwise**;
- **adjacency** — thin straight radial lines join adjacent planets, thick
  irregular hand-drawn borders separate non-adjacent ones (p6).

Planets are listed in increasing angle (clockwise) within each wedge, which is
the order `CLUSTERS` uses:

| cluster | planet 1 | planet 2 | planet 3 |
|---|---|---|---|
| 1 | Weapon 2 | Fuel 1 | Material 2 |
| 2 | Psionic 1 | Weapon 1 | Relic 2 |
| 3 | Material 1 | Fuel 1 | Weapon 2 |
| 4 | Relic 2 | Fuel 2 | Material 1 |
| 5 | Weapon 1 | Relic 1 | Psionic 2 |
| 6 | Material 1 | Fuel 2 | Psionic 1 |

Two independent cross-checks hold: the type totals come to **Material 4, Fuel 4,
Weapon 4, Relic 3, Psionic 3 = 18**, and every cluster carries three distinct
types. `components.test.ts` asserts both, plus the 26-slot total, so a future
correction to one planet cannot silently break the distribution.

One caveat on provenance: the page-4 illustration in the PDF is cropped at the
right edge, so clusters 2 and 3's outermost planets were read from a
photographed board rather than the PDF.

To correct: edit `CLUSTERS` in `map.ts` — each entry lists its 3 planets as
`{ type, slots }`.

## Closed: the Court

All 31 cards — name, suit, raid cost and verbatim rules text — are transcribed
from card images in `court.ts`. The printed numbers run 01–25 Guild and 26–31
Vox; seven of those numbers are legible in the rulebook itself (01 Loyal
Engineers, 03 Material Cartel, 04 Admin Union, 09 Shipping Interest, 11 Arms
Union, 15 Loyal Marines, 18 Secret Order) and all seven agree with the
transcription, which pins the ordering.

**Every printed ability is now dispatched.** `POWER_STATUS` in `court.ts` records
each card as `full`, `partial` or `none`; all 31 read `full`, and
`powers.test.ts` asserts `UNIMPLEMENTED_POWERS` is empty, so a rollback would
fail the suite rather than quietly contradict this file.

The abilities fall into the kinds the rulebook names (p20):

| kind | cards | example |
|---|---|---|
| `Prelude:` abilities | 19 | *Silver-Tongues* — discard to steal a Guild card or resource |
| new actions `Name (Standard):` | 5 cards, 6 actions | *Prison Wardens* — Pressgang (Build), Execute (Influence) |
| passive modifiers | 17 | *Gatekeepers* — collect 2 more dice when battling in a gate |
| Vox `When Secured:` | 6 | *Populist Demands* — declare any ambition |

(Cards can appear in more than one row.)

Four of them needed structure the engine did not have, and that structure is
now part of the state rather than bolted onto a card:

- **the Cartels** hold their resource type's whole supply on the card
  (`GameState.cartel`). While one is in play that type's general supply is
  empty, returned tokens flow onto the card, the holder counts them toward
  Tycoon but cannot spend them, and Rivals lose all of that type after scoring.
- **the Unions** attach to a face-up played action card (`GameState.unions`) and
  hand it to their owner when the round ends.
- **Skirmishers** needs a decision *after* the dice are rolled, so a battle now
  has a `battleReroll` phase between the roll and hit assignment.
- **Farseers** reveals one named Rival's hand, which is the only place the
  engine deliberately opens the imperfect-information boundary. It is two
  decisions (`peekTarget`, then `peekSwap`) precisely so that committing to a
  target happens *before* the reveal — enumerating swaps across all Rivals at
  once would leak every hand instead of the one the card names. `observe()`
  reveals exactly the chosen hand, and only while the swap is pending.
- **the Vox cards** resolve on being secured, which can happen mid-turn or
  mid-Ransack, so `pendingVox` overlays the current phase and both `afterAction`
  and `settleBattle` stall until it is answered.

Three of those raised questions the cards do not answer; all three are recorded
as [engine rulings](RULES.md#11-engine-rulings).

## 3. Setup cards

The box has 12 setup cards — 4 each for 2, 3 and 4 players — and their contents
are not in the rulebook. `SETUP_DECK` in `setup.ts` holds a reconstructed deck of
the same shape, and a game plays step I as written: shuffle the cards for your
player count, draw one, take positions on it.

**How the four were chosen.** Not by hand and not uniformly. `tools/pick-setups.ts`
scores 20,000 legal openings for balance — equal building capacity across
players, two distinct resource types each, and no two players crowded into
neighbouring clusters — and keeps the best four per count, requiring them to
differ in which clusters they take out of play. The 2-player cards score
perfectly balanced; 3 and 4 players cannot, because 4 or 5 live clusters will
not seat that many players an equal distance apart. That is the map's geometry,
not a defect in the search.

Every stated setup rule holds, asserted per card in `rules.test.ts`: the right
number of out-of-play clusters, each taking its gate and the 3 planets touching
it (p4 step J); one A, one B and one C per player, two Cs at 2 players; A and B
always planets, since they take a city and a starport and their printed resource
is gained at step O; nothing in a dead cluster; no system shared.

### Two things the card does not decide

- **Which player takes which position.** The card labels its positions 1A/1B/1C,
  2A/2B/2C and so on, and nothing says seat 0 is position 1. `drawSetup` assigns
  players to positions at random, so a human in seat 0 does not always open in
  the same corner of the same card.
- **Turn order**, which comes from the initiative marker and is drawn at setup.

Both are uniform over 2000 draws at every player count.

### The measurement mode

`SetupMode` is `deck` by default — the game as played, four boards per count.
`draw` invents a fresh legal opening per seed instead, reaching about 3000
boards per count, and is exposed as `--free-setup`.

The distinction earns its keep, because **few openings flatter results**. On the
six fixed rotations this project used originally, `greedy` beat `mc` by
+31.7 ±15.1. On freely drawn openings the same matchup is +13.3 ±17.1, and on
the four setup cards +18.3 ±18.3 — neither separated. Nothing about either agent
changed. Use `deck` to ask "who wins Arcs", and `draw` when the opening should
be a nuisance variable rather than part of the game.

### What one printed card tells us

The rulebook's component photo (p3) shows *2 Players – Frontiers* legibly at
400dpi, and it corrects an assumption an earlier generator made: `1A` and `1B`
sit in **different clusters**, and the two `1C`s are scattered. Real setups
spread a player across the map rather than giving them a home cluster, and the
reconstruction follows that.

To use the printed cards, replace `SETUP_DECK` with the real 12.

## 4. Player board economy

Reconstructed: 5 city slots and 6 resource slots with raid costs
`[1, 1, 2, 2, 3, 3]`; three resource slots open at setup (covering the two
starting resources, p5 step O); building cities uncovers further resource slots
and the "+2 to won ambitions" and "+3 to won ambitions" bonuses (p18).

Which city slot uncovers which reward is the invented part. See
`PLAYER_BOARD.citySlots` in `playerBoard.ts`.
