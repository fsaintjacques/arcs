# Findings

Research notes from building the engine and the first bots. Headline numbers
are in the README; this keeps the evidence, the negative results, and the
methodology traps that cost real time.

Everything here describes **this engine's Arcs**. All 31 Court cards are real
data with every printed ability dispatched, and the map's planet types and
building slots are transcribed from the printed board; the setup cards and a few
component details are still reconstructed (see [DATA-GAPS.md](DATA-GAPS.md)), so
treat the strategic conclusions as provisional.

## Methodology traps

### Rotating seats is not enough — permute them

The first tournament runner rotated agents through seats, which is the obvious
way to cancel seat advantage. It does not. Rotation preserves the agents'
*cyclic order*, and Arcs' turn order is cyclic: the player seated immediately
after a weak player gets to Surpass cheap leads all game.

Two **identical** greedy agents, 60 games, 3 players with a `random` third:

| seating | agent A | agent B |
|---|---|---|
| rotated | 78.3% | 21.7% |
| permuted | 50.0% | 50.0% |

Under rotation, agent A always sat immediately after the random player. Every
comparison run before this fix was measuring seat relationship, not strength.
`simulate()` now cycles all `n!` permutations, and a test asserts that
identical agents come out even.

### A one-step eval scores an action before its consequences exist

`battle` opens a chance node — the dice have not been rolled when the action is
applied — so a naive one-step lookahead scores "I battled" as identical to "I
did nothing", minus a pip. The bot correctly concluded that battling is a waste.

Measured over 10 two-player games: battle was **offered 163 times and taken 0**.

Playing the cascade out before scoring (`settle()` in `greedy.ts`: resolve
chance from the RNG, resolve the agent's own follow-up decisions with the same
greedy rule) took that to **104 of 164**, and moved greedy from never taking a
trophy to ~4 per game. This was worth more than any weight tuning so far, and
it is the kind of bug that looks like a strategy preference rather than a
defect — the bot was not "peaceful", it was blind.

The same trap applies to Move, which can open a Catapult decision.

### Determinized search needs a deterministic action trim

Arcs offers hundreds of legal actions at some nodes: a Move pip enumerates
every ship count on every edge, a Battle enumerates every dice split. The
search needs to trim, but the first trim sampled randomly — and because
determinized ISMCTS revisits a node under a *different* sampled world each
iteration, a random trim gave the node a different child set every visit. Its
statistics described no particular decision, and a cached action list went
stale and threw (`unhandled battle action move`).

`narrow()` is now deterministic and round-robins across action kinds, so no
action type is invisible however many `move` variants crowd the list, and
legality is re-derived on every visit rather than cached.

## The catapult can loop forever

The rules say a catapult keeps moving "until they move to a gate controlled by
anyone else or they move to any planet" (p13). Between two uncontrolled gates
that is a legal infinite loop — a human would never do it, a bot maximising a
noisy evaluation does it immediately, and the first greedy game hung.

The engine forbids re-entering a system the same catapult has already passed
through. Revisiting can only undo progress, so no legal outcome is lost. This
is one of five documented [engine rulings](RULES.md#11-engine-rulings).

## Wrong dice changed the ladder, not just the numbers

The first version of the engine inferred the assault and raid die faces from
odds quoted in reviews, because the rulebook prints only the symbols. The
inferred tables satisfied every quoted statistic — 5-of-6 faces with a hit,
2-of-6 with two hits, self-hits on half — and were still wrong about *which
symbols share a face*. The official aid booklet prints all six faces of each
die; the real ones are:

| die | faces |
|---|---|
| Skirmish | 3× 1 hit, 3× blank (the inference was right) |
| Assault | 2 hits · 2 hits + self · **1 hit + intercept** · 1 hit + self · 1 hit + self · blank |
| Raid | **2 keys + intercept** · 1 key + self · **1 building hit + 1 key** · building + self · building + self · intercept |

The assault die has *identical* expected hits and self-hits per die either way.
What changed is correlation: the intercept face also deals a hit, two-hit faces
no longer always cost you a ship, and the raid die trades a self-hit face for a
second intercept face.

That was enough to move the ladder. Against the inferred dice, `mcts` and
`greedy` split 50/50 over 60 games; against the real ones the point estimate
went to 65/35 for `mcts` (and then back the other way once card abilities
landed — see below). `random+` vs `random` moved too, from 65/35 to an even
split.

The lesson is that aggregate statistics are not a substitute for the joint
distribution: a bot that decides *how many of which die to collect* is reading
exactly the structure that averages throw away.

## The ladder

Current numbers: corrected dice, the transcribed map, and the complete Court —
all 31 card abilities dispatched.

2 players:

| matchup | games | win % | mean Power |
|---|---|---|---|
| `random+` vs `random` | 40 | 52.5 / 47.5 | 4.3 / 6.3 |
| `greedy` vs `random+` | 40 | 100.0 / 0.0 | 43.3 / 1.5 |
| `greedy` vs `mc` | 40 | 52.5 / 47.5 | 23.9 / 23.4 |
| `greedy` vs `mcts` | 60 | 28.3 / **71.7** | 21.0 / 25.5 |

3 players:

| field | games | win % |
|---|---|---|
| `greedy`, `mc`, `random` | 60 | 56.7 / 43.3 / 0.0 |

An earlier 4-player run put two identical greedy seats at 43.8% and 41.7% — a
useful check that permuted seating is doing its job.

### `mcts` finally separated from `greedy`

`greedy` vs `mcts`, same agents, same seeds, 60 games each, across five states of
the rules:

| rules state | greedy | mcts | interval covers 50%? |
|---|---|---|---|
| inferred die faces | 50.0 | 50.0 | yes |
| corrected die faces | 35.0 | 65.0 | yes |
| + first Guild abilities | 55.0 | 45.0 | yes |
| + transcribed map | 41.7 | 58.3 | yes |
| + the complete Court | 28.3 | **71.7** | **no** |

The first four rows all sat inside a ±12.6 interval, and this file has said four
times that these two agents were closer together than 60 games could resolve.
That was the right call then and it is worth keeping the record of it.

The fifth row is different: 71.7 ± 11.4 gives [60.3, 83.1], which excludes 50%.
After four readings of noise, **the same batch size now shows a real gap**, and
`mcts` also passes `greedy` on mean Power for the first time (25.5 vs 21.0),
which had been the one stable difference between them.

The most likely reason is that a complete Court is the part of Arcs that rewards
lookahead. Card abilities are conditional, sequenced, and interact — a Prelude
that places ships changes what a Battle can do later in the same turn; securing a
Vox card opens a decision mid-turn. A one-step evaluation prices those at
whatever the position looks like immediately afterwards, while a tree can see the
follow-up. Every earlier version of the rules gave the search less of this to
find. It is a hypothesis, not a demonstration — the honest test is to re-run the
matchup with abilities disabled, which is a switch the engine does not currently
have.

What it also means is that **greedy is now the weaker measuring stick**, and the
`eval.ts` blind spot below is part of why.

### Fixing the map moved the ladder more than any agent change has

The map correction only changed *which resource each planet yields and how many
buildings fit on it* — no rule, no agent, no evaluation weight. It moved
`greedy` vs `mc` from 67.5/32.5 to 45.0/55.0, a 22.5-point swing, and reversed
the heads-up result.

The mechanism is visible in the type distribution. The invented map spread the
types near-evenly and gave weapon planets deliberately scarce placement; the
printed map has **Weapon ×4 and Relic/Psionic ×3**, so the two cheapest
ambitions to contest — Keeper and Empath — sit on the *scarce* resources, and
Weapon worlds, which score no ambition at all, are the *common* ones. A bot
whose evaluation prices resources uniformly is now systematically overvaluing a
third of the board.

`greedy` still wins the 3-player field, so the heads-up flip was not a clean
reversal of strength — it was two agents trading places inside the noise band
while the terrain under both of them changed. The lesson matches the dice one:
**component data is not a detail that rounds out; it is an input the search reads
directly.** Three of the five readings above moved on data, not code.

It also retired a finding that had been stable across every previous version:
`mcts` no longer scores less Power than `greedy`. Whatever was driving that gap
was a property of the invented economy.

### `random+` is not an improvement

"Never end a turn with actions unspent, never burn a card to seize" sounds
strictly better than uniform random. Over three 40-game readings it has come out
52.5/47.5, then 62.5/37.5, then 52.5/47.5 again — all inside the ±15.5 interval.
Spending every pip on a randomly chosen action is not worth much when the action
is random. It remains useful as the rollout policy because it keeps rollouts
moving, but 40 games cannot tell it apart from uniform random, and the readings
straddling the midline are the evidence for that rather than against it. In the
latest run `random` actually finished with *more* mean Power (6.3 vs 4.3) while
winning less often, which is what noise at this level looks like.

### Flat Monte-Carlo: the multiplayer result survived, the heads-up one did not

`mc` samples worlds and plays each candidate action out several times, which is
strictly more information than greedy's single settled lookahead. On the invented
map it lost 32.5/67.5 heads-up; it has since read 45.0/55.0 and now 52.5/47.5 —
a dead heat. What did *not* change is the 3-player field, where it has come
second to `greedy` every time (43.3/56.7 now).

The structural argument for why `mc` should be weaker still holds: it has no
tree, so it cannot see its own follow-up pips. An Arcs turn is a *sequence* of
1–4 dependent actions (build a starport, then build a ship at it; move in, then
battle), and evaluating the first action of that sequence against a random
continuation prices the setup at close to nothing.

But the heads-up number no longer supports that argument, and the honest reading
is that the argument was over-credited: a 22.5-point swing from a data edit means
the original 67.5 was measuring the invented economy as much as the search
shape. The multiplayer field is the more durable signal, and it is the weaker
claim — `mc` is second, not beaten.

### Greedy builds more cities; the Power gap did not survive

Across every earlier version of the rules `mcts` finished with lower mean Power
than `greedy` while staying level on wins, and that looked like a structural
consequence of valuing rollouts on final standing rather than on Power: the
search has no reason to prefer a position that scores 40 and loses over one that
scores 24 and wins, and greedy's evaluation is Power-shaped and cannot express
that difference.

On the printed map the gap closed, and with the complete Court it has **reversed**
— `mcts` now scores 25.5 to `greedy`'s 21.0 while also winning 71.7%. What
survives is the city count: `greedy` builds 4.4 to `mcts`'s 4.1, and has built
more in every single run, which is the part its Power-shaped evaluation actually
rewards.

So the mechanism was wrong even though it fit four consecutive datasets. The
Power gap was an artefact of the invented economy, not a consequence of valuing
rollouts on standing. The narrower claim — that greedy over-builds cities because
that is what its evaluation can see — is the one that held.

Remaining levers for the search, roughly in order of expected value:

1. **Iterations vs branching.** 300 iterations against a node with 100+ legal
   actions barely leaves the root, and the complete Court made some nodes wider
   still: Pressgang alone enumerates every multiset of resources it could gain.
   `narrow()` caps branching at 12, which helps, but the trim is uniform across
   kinds rather than informed.
2. **Rollout depth.** 30 decisions rarely reaches a chapter break, so most
   rollouts are valued by the same heuristic greedy uses.
3. **A greedy rollout policy** instead of `random+`, so the sampled
   continuations resemble the play the values are meant to describe.

## Finishing the Court found a hole in the bots, not the engine

All 31 abilities are now dispatched. Instrumenting 40 three-player all-`greedy`
games shows most of them reaching real play: the Cartels hold a supply in 450
positions, Farseers peeks 16 times and swaps on 14 of them, Silver-Tongues steals
a Guild card, Skirmishers rerolls, and all five Vox cards that need a decision
resolve. So the dispatch is live rather than merely present.

Three abilities are **offered constantly and never once taken**:

| ability | offered | taken |
|---|---|---|
| Farseers' Prelude (discard n, redraw n+1) | 998 | 0 |
| attaching a Union to a played card | 119 | 0 |
| Execute (Captives → Trophies) | 15 | 0 |

That is not a coincidence of three cards; it is one blind spot showing up three
times. `eval.ts` scores a position by board and Power, and has **no term for hand
quality** — so trading cards for better cards, or attaching a Union to draw a
card next round, both evaluate as approximately zero, and the bot correctly
declines to pay anything for them. Execute is the same shape from the other
side: it converts a Tyrant count into a Warlord count, and the evaluation prices
the two alike, so the swap looks like pure loss of optionality.

This is the same class of bug as the cascade-settling one further up — an ability
that looks like a strategy preference ("the bot doesn't like card filtering")
but is actually an evaluation that cannot represent the payoff. The difference is
that this time the engine is right and the agent is wrong, which is only visible
because the abilities are enumerated and countable.

Concretely: a hand-quality term (cards held, pip total, suit spread relative to
the current lead) is now the highest-value thing to add to `eval.ts`, ahead of
weight tuning on the terms already there.

## Open questions

- **A hand-quality term in `eval.ts`.** The highest-value item on this list, for
  the reason measured above: three abilities are offered over a thousand times
  between them and never taken, because the evaluation has no way to say that a
  better hand is worth something.
- **Ambition timing.** No bot yet reasons about *when* to declare. The zero
  marker makes a declared lead card trivially surpassable, so declaring costs
  the initiative — a tradeoff none of the current evaluations model. Two cards
  now bear directly on it: Secret Order and Galactic Bards both suppress the zero
  marker, so a bot holding either should declare far more freely, and none of
  them notices.
- **Weight tuning.** `eval.ts` weights are hand-set, and the map correction gave
  a concrete reason to expect gains: Relic and Psionic are the scarce planet
  types but the evaluation prices all five resources alike, and Weapon — the
  commonest type — scores no ambition at all. A CEM or paired-seed grid search
  over the weights is the obvious next step.
- **Batch size.** Four of the five `greedy`/`mcts` readings sat inside the
  interval, and only the fifth cleared it. `greedy` vs `mc` is still unresolved
  after three readings. Paired-seed variance reduction is the cheap fix and is
  worth more than another 60 games.
- **An abilities-off switch.** The claim that a complete Court is what let `mcts`
  separate is untested, because there is no way to run the same batch with the
  abilities disabled. A variant flag that keeps the card data but drops
  `power`/`vox` dispatch would turn that hypothesis into a measurement.
- **Learned value function.** The state is large but regular (24 systems ×
  pieces, 5 ambitions, hand shape); a TD(λ) afterstate net is the natural
  follow-up now that the Court is complete.
