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
| 2 | Player board economy | `src/engine/playerBoard.ts` | low |

**Closed:** battle die faces (official aid booklet p3 prints all six faces of
each die), the Court card data (names, suits, raid costs and verbatim text for
all 31 cards), **every Court card ability**, all 31 now dispatched, **the map**
in full — planet types, building-slot counts and every one of the 18 borders
round the planet ring — and **the 12 setup cards**.

Only two entries are left, and neither is load-bearing: the game plays the same
whatever the ambition markers flip to, and the player board's reward layout is a
late-game detail.

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

## Closed: the map

Nothing about the map is reconstructed any more. 6 clusters of 1 gate + 3
planets, the gate ring, out-of-play clusters and path markers all come straight
from the rules; the planet types and building-slot counts are transcribed from
the printed board; and the 18 borders round the planet ring are transcribed from
the setup cards.

### The planet transcription

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

### The border transcription

The 18 planets are a **ring**, not six separate rows, so a cluster boundary is
just another border between two planets. The rulebook is explicit that this is
what decides adjacency — "A system is adjacent to other systems sharing a thin
border… Planets separated by a thick, irregular border are not adjacent" (p6) —
and it never says a planet's neighbours have to be in its own cluster.

The planet art on the printed board makes the borders hard to trace, but the
**setup cards draw the same map schematically** with nothing else on it. Taking
the pixel-wise **median of all 12** erases each card's own labels and
out-of-play shading and leaves the bare border drawing:

```bash
convert piece_*.jpg -evaluate-sequence median median.png
```

Scanning circles around the map centre and measuring each radial border's width
and how far it wanders in angle separates them into three clean tiers:

| tier | count | width at r=220px | angular wander | reading |
|---|---|---|---|---|
| intra-cluster | 12 | 5.0–6.8 px | 0.09–1.05° | thin — adjacent |
| cluster outline | 2 | 10.0, 11.1 px | 0.11°, 0.34° | thin — adjacent |
| hand-drawn | 4 | 20.2–24.5 px | 0.52–0.89° | thick, irregular — **not** adjacent |

So **four** of the six cluster boundaries carry the thick irregular border and
**two** do not. The two that don't are drawn dead straight, only a little
heavier than the rest — a heavier cluster outline, not the irregular border,
which is unmistakable at 4× the width. The joins are:

- **2.3 – 3.1**
- **5.3 – 6.1**

Three things agree on the orientation: the top sector of the median schematic
sits at ~11° clockwise of vertical, the printed board's gate "1" sits at ~11.7°,
and gate numbering runs clockwise on both. The cluster boundaries then fall at
52°, 97°, 147°, 231°, 276° and 331°, and the two thin ones are 97° (2|3) and
276° (5|6).

Within a cluster all 12 borders are thin, so each cluster's planets are a path
(1–2 and 2–3 adjacent, 1–3 not) — which is what the engine already assumed.

To correct: edit `JOINED_BOUNDARIES` in `map.ts`.

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

## Closed: the setup cards

`SETUP_DECK` in `setup.ts` is the printed 12 — 4 each for 2, 3 and 4 players —
transcribed from the card faces. A game plays step I as written: shuffle the
cards for your player count, draw one, take positions on it.

### How they were read

Each card draws the map schematically — the same 18 planet slices and 6 gate
sectors as the board, with no planet art — and prints a position label ("2B",
"1C") in each system a player starts in. So reading a card is a geometry
problem, not an OCR one: the border angles were measured once (above), which
fixes every sector's span, and a label's angle and radius then name its system.

`tools/read-setups.mjs` redraws the sector names over a card so any entry can be
re-checked by eye:

```bash
node tools/read-setups.mjs "2 Players - Frontiers.jpg" annotated.png
```

Three things check the result:

- **Two independent images of the same card agree.** The rulebook's component
  photo (p3) prints *2 Players – Frontiers* legibly at 400dpi, and every one of
  its eight labels lands where the card scan puts it.
- **Building capacity comes out level.** Each player's A and B planets carry 3
  building slots between them on 9 of the 12 cards, and are off by one on the
  other 3. The transcription never used the slot counts — those were read off
  the board months earlier — so two independent readings agreeing on a design
  invariant is a real check.
- **Every stated setup rule holds**, asserted per card in `rules.test.ts`: the
  right number of out-of-play clusters, each taking its gate and the 3 planets
  touching it (p4 step J); one A, one B and one C per player, two Cs at 2
  players; A and B always planets, since they take a city and a starport and
  their printed resource is gained at step O; nothing in a dead cluster; no
  system shared.

One assumption the real cards overturned: **C positions are not always gates.**
The reconstruction made them so; *2 Players – Mix Up 1* puts a 1C on planet 6.1
and *2 Players – Frontiers* puts a 1C on planet 3.3. C only ever takes ships, so
nothing requires it to be a gate.

### Which player takes which position

Not the card's business, and not a separate random draw either — it follows the
initiative marker: "The player with the initiative marker does this: place 3
ships and 1 city in the system marked 1A… Going clockwise from the player with
initiative, the 2nd, 3rd, and 4th players set up in the same way in the systems
marked 2A–C, 3A–C, and 4A–C" (p5 step N). Initiative goes to a random player at
step B, so the assignment is random, but it is a random **rotation** of the
seats rather than a free permutation, and position 1 always belongs to the
player who moves first.

An earlier pass randomised position independently of initiative, which
over-randomises: it breaks the link between opening first and opening on
position 1. `rules.test.ts` now asserts the rotation directly.

### The measurement mode

`SetupMode` is `deck` by default — the game as played, four boards per count.
`draw` invents a fresh legal opening per seed instead, reaching about 3000
boards per count, and is exposed as `--free-setup`.

The distinction earns its keep, because **few openings flatter results**. On the
six fixed rotations this project used originally, `greedy` beat `mc` by
+31.7 ±15.1. On freely drawn openings the same matchup is +13.3 ±17.1 — not
separated. Nothing about either agent changed. Use `deck` to ask "who wins
Arcs", and `draw` when the opening should be a nuisance variable rather than
part of the game.

To correct a misread label: edit `SETUP_DECK` in `setup.ts`. Systems are engine
ids (`cluster * 4 + slot`, slot 0 the gate), so the printed "3.1" is 9 and the
printed gate "5" is 16.

## 2. Player board economy

Reconstructed: 5 city slots and 6 resource slots with raid costs
`[1, 1, 2, 2, 3, 3]`; three resource slots open at setup (covering the two
starting resources, p5 step O); building cities uncovers further resource slots
and the "+2 to won ambitions" and "+3 to won ambitions" bonuses (p18).

Which city slot uncovers which reward is the invented part. See
`PLAYER_BOARD.citySlots` in `playerBoard.ts`.
