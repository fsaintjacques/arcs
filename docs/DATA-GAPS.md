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
| 2 | Map planet types, slots, adjacency | `src/engine/map.ts` | low — structurally faithful, layout invented |
| 3 | Setup cards | `src/engine/setup.ts` | low — generated symmetrically, not the printed 12 |
| 4 | Guild / Vox ability *dispatch* | `src/engine/court.ts` | data exact; 13 of 31 abilities fully dispatched |
| 5 | Player board economy | `src/engine/playerBoard.ts` | low |

**Closed:** battle die faces (official aid booklet p3 prints all six faces of
each die) and the Court card data (names, suits, raid costs and verbatim text
for all 31 cards).

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
their gate; out-of-play clusters and path markers.

**Invented**: which planet type sits in which of the 18 planet slots, how many
building slots each planet has (the rulebook says 1 or 2), and which
intra-cluster planet pairs share a thin border. The engine models each cluster's
planets as a path — planet 0–1 and 1–2 adjacent, 0–2 not — which satisfies
"adjacent to one or both neighbouring planets" (p6).

The planet-type table distributes the 5 types over 18 planets as evenly as the
count allows and gives every cluster three distinct types, so no cluster is a
monoculture. It is *a* legal Reach, not *the* Reach.

To correct: replace `CLUSTERS` in `map.ts` — each entry lists its 3 planets as
`{ type, slots }`.

## 3. Setup cards

The box has 12 setup cards (4 each for 2, 3 and 4 players). Their contents are
not in the rulebook. `setup.ts` instead **generates** setups that obey every
setup rule the rulebook does state:

- the correct number of out-of-play clusters (2 for 2–3 players, 1 for 4),
- one A, one B and one C system per player (two C systems at 2 players),
- starting systems spread symmetrically around the ring, never in an
  out-of-play cluster, and never shared between players,
- A and B are always planets (they receive a city and a starport).

`--setup <n>` / `setupIndex` selects among the generated layouts, so batches
still vary. To use the printed cards instead, replace `generateSetup()` with a
literal table of the 12 setups.

## 4. Guild and Vox card abilities

**The card data is no longer a gap.** All 31 cards — name, suit, raid cost and
verbatim rules text — are transcribed from card images in `court.ts`. The
printed numbers run 01–25 Guild and 26–31 Vox; seven of those numbers are
legible in the rulebook itself (01 Loyal Engineers, 03 Material Cartel, 04
Admin Union, 09 Shipping Interest, 11 Arms Union, 15 Loyal Marines, 18 Secret
Order) and all seven agree with the transcription, which pins the ordering.

What remains is **dispatching the rest of the abilities**. `POWER_STATUS` in
`court.ts` records each card as `full`, `partial` or `none` and names what is
missing; tests assert every card carrying an ability is classified and that
anything short of `full` says why, so this document cannot drift.

As of now: **13 full, 6 partial, 12 none**.

| status | cards |
|---|---|
| full | the 5 Loyal cards, Mining Interest, Shipping Interest, Gatekeepers, Lattice Spies, Secret Order, Silver-Tongues, Sworn Guardians, Relic Fence |
| partial (Prelude works, rest does not) | both Cartels, Prison Wardens, Skirmishers, Court Enforcers, Elder Broker |
| none | the 4 Union cards, Farseers, Galactic Bards, and all 6 Vox |

Everything else about every card already works exactly as printed: suit counts
toward Tycoon / Keeper / Empath, Weapon cards count toward nothing, raid costs
gate what an attacker can steal, and Outrage discards cards by suit.

The abilities fall into the three kinds the rulebook names (p20):

| kind | cards | example |
|---|---|---|
| `Prelude:` abilities | 13 | *Silver-Tongues* — discard to steal a Guild card or resource |
| new actions `Name (Standard):` | 5 cards, 6 actions | *Prison Wardens* — Pressgang (Build), Execute (Influence) |
| passive modifiers | 17 | *Gatekeepers* — collect 2 more dice when battling in a gate |
| Vox `When Secured:` | 6 | *Populist Demands* — declare any ambition |

(Cards can appear in more than one row.)

The two structural pieces still missing are the **Unions** (attach to a played
card, draw it at the end of the round) and the **Cartels** (hold a resource
supply on the card) — both need state that does not exist yet. The Vox cards
each need a `When Secured` decision node.

## 5. Player board economy

Reconstructed: 5 city slots and 6 resource slots with raid costs
`[1, 1, 2, 2, 3, 3]`; three resource slots open at setup (covering the two
starting resources, p5 step O); building cities uncovers further resource slots
and the "+2 to won ambitions" and "+3 to won ambitions" bonuses (p18).

Which city slot uncovers which reward is the invented part. See
`PLAYER_BOARD.citySlots` in `playerBoard.ts`.
