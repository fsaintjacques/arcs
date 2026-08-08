# Component data not in the rulebook

The base rulebook (April 11 2024) documents every *rule* the engine needs, but
several pieces of *component data* live only on the physical components. Each
one below is reconstructed, isolated in a single data module, and marked in
code with `// DATA-GAP:` so it can be corrected with a data edit and no logic
change.

Nothing here affects the shape of the engine — the decision process, the action
space, and the agent API are all independent of these values.

| # | Datum | Module | Confidence |
|---|---|---|---|
| 1 | Assault & raid die faces | `src/engine/dice.ts` | high — pinned by published probabilities |
| 2 | Ambition marker reverse sides | `src/engine/ambitions.ts` | medium — one of three confirmed |
| 3 | Map planet types, slots, adjacency | `src/engine/map.ts` | low — structurally faithful, layout invented |
| 4 | Setup cards | `src/engine/setup.ts` | low — generated symmetrically, not the printed 12 |
| 5 | Guild / Vox card texts | `src/engine/court.ts` | none — shipped as vanilla cards |
| 6 | Player board economy | `src/engine/playerBoard.ts` | low |

---

## 1. Battle die faces

The rulebook (p14) gives the five symbols and the resolution order, but not the
face distributions. Reconstructed from the rulebook semantics plus the
probabilities quoted in published reviews:

- Skirmish: "a single hit on three faces, blank on the other three" — **confirmed**.
- Assault: "5 in 6 chance of at least 1 hit", "1 in 3 of 2 hits", "50% chance of
  a hit in return", "1 in 6 chance" of intercept.
- Raid: "half the faces include key icons", "50% chance of doing one point of
  damage to a building", "carry the most risk of self-hits", no damage to ships.

The tables in `dice.ts` are the unique simple solution satisfying all of those
constraints simultaneously. **Assault is high confidence; raid's split of
self-hits and the intercept face is the weakest link.**

To correct: edit `ASSAULT_FACES` / `RAID_FACES` / `SKIRMISH_FACES`. Each face is
`{ selfHits, intercept, hits, buildingHits, keys }`.

## 2. Ambition marker reverse sides

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

## 3. Map layout

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
`{ type, slots }` and optionally overrides `planetAdjacency`.

## 4. Setup cards

The box has 12 setup cards (4 each for 2, 3 and 4 players). Their contents are
not in the rulebook. `setup.ts` instead **generates** setups that obey every
setup rule the rulebook does state:

- the correct number of out-of-play clusters (2 for 2–3 players, 1 for 4),
- one A, one B and one C system per player (two C systems at 2 players),
- starting systems spread symmetrically around the ring, never in an
  out-of-play cluster, and never shared between players,
- A and B are always planets (they receive a city and a starport).

`--setup <n>` / `setupIndex` selects among the generated layouts, so batches
still vary. To use the printed cards instead, replace `generateSetups()` with a
literal table of the 12 setups.

## 5. Guild and Vox card texts

The 25 Guild and 6 Vox cards each carry bespoke rules text; none of it is in the
rulebook. The engine ships the Court as a **structurally complete but
mechanically vanilla** deck:

- 25 Guild cards with a suit (one of the 5 resource types) and a raid cost, so
  influencing, securing, raiding, Outrage discards and the Tycoon / Keeper /
  Empath ambition counts all work exactly as written;
- 6 Vox cards that resolve immediately on being secured and are discarded.

What is missing is only the per-card *special powers* (new actions, modifiers,
Prelude abilities). `court.ts` defines a `CourtCardDef` with optional
`whenSecured`, `prelude` and `modifiers` hooks, and `effects.ts` dispatches
them, so adding the real cards is a data exercise.

**This is the largest gap.** Guild card powers are a real part of base-game
strategy, so bot results from this engine describe a simplified Arcs.

## 6. Player board economy

Reconstructed: 5 city slots and 6 resource slots with raid costs
`[1, 1, 2, 2, 3, 3]`; two resource slots open at setup (matching the two
starting resources, p5 step O); building cities uncovers further resource slots
and the "+2 to won ambitions" and "+3 to won ambitions" bonuses (p18).

Which city slot uncovers which reward is the invented part. See
`PLAYER_BOARD.citySlots` in `playerBoard.ts`.
