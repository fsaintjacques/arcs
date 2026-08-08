# Findings

Research notes from building the engine and the first bots. Headline numbers
are in the README; this keeps the evidence, the negative results, and the
methodology traps that cost real time.

Everything here describes **this engine's Arcs**. All 31 Court cards are real
data with every printed ability dispatched, and the map's planet types and
building slots are transcribed from the printed board. Opening positions are a
random legal draw rather than the printed 12 setup cards, and a few component
details are still reconstructed (see [DATA-GAPS.md](DATA-GAPS.md)), so treat the
strategic conclusions as provisional.

## Methodology traps

### The harness beat an agent with a copy of itself, by 14 points

The worst measurement bug in the project, and it survived the seat-permutation
fix below because it hides *behind* it.

`simulate()` advanced the seed **and the setup index on every game**, while
cycling seatings through the `n!` permutations. At 2 players that aliases
seating to setup parity exactly: agent 1 sat in seat 0 for every even setup,
agent 0 for every odd one. The permutations never met the same deal, so they
cancelled nothing — each agent simply drew a different, systematically
different, mix of starting positions.

Two **identical** greedy agents, 8 replications of 40 games each:

| scheme | mean win-share difference | sd across replications |
|---|---|---|
| paired (now) | **0.00** | 0.00 |
| unpaired (before) | **−13.75** | 12.46 |

Every replication of the paired scheme returns exactly zero, which is what
common random numbers should do: play the same deal from both seats and one
agent's good luck is the other's, precisely. The old scheme has an agent losing
to a copy of itself by 14 points, in 7 of 8 replications.

That is **bias, not variance**, and it is larger than most of the effects this
file has reported. Every 2-player head-to-head number here came off that
harness.

The fix is to hold the deal fixed across a block of `n!` seatings and round the
batch up to whole blocks. `pairedStats()` then works from within-block
differences, so the unit of observation is a deal rather than a game.

**What pairing did not buy.** It is worth being exact, because the expected
benefit did not materialise: the spread of the estimate is essentially
unchanged (sd 9.49 paired against 9.49 unpaired for `greedy` vs `greedy-flat`;
5.67 against 6.59 for the two random agents). Common random numbers reduce
variance only when the two agents' trajectories stay correlated, and in a game
with this much branching they diverge within a few decisions. The win is
removing the confound. The intervals are as wide as they ever were, and the
answer to "how do I resolve a 5-point difference" is still "play more games".

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

### The sampler dealt out cards it had watched being played

Found while building the belief model below, and worth more than the model was.

`determinize()` treated a card as accounted for if it was in the observer's
hand or on the table *this round*. Cards played face up in **earlier** rounds of
the same chapter were forgotten, so the sampler cheerfully dealt them back into
someone's hand. Measured over 30 games: **65% of sampled worlds contained at
least one card the observer had personally watched being played**, 1.56 of them
on average.

Every search agent was therefore spending part of its budget on worlds that
could not exist. The fix is a public memory (`GameState.revealed`) reset each
chapter, which `observe()` passes through and `determinize()` is bound by. It
takes the impossible-card count to zero.

The general lesson is that the imperfect-information boundary needs a test that
asks "could this world actually be true", not just "does the observer's own
hand survive the round trip". The latter passed throughout.

## Inferring hands from play: a negative result

Arcs looked unusually friendly to belief modelling. The hidden state is tiny —
at 3 players the deck is 20 cards, you hold 6, and the remaining 14 split
6/6/2 — and a follower may Surpass only with the lead suit and a higher number
(p10), so a player who Copies or Pivots instead appears to be advertising that
they hold no such card. Uniform determinization throws all of that away.

**It does not work.** The signal exists, is correctly signed, and is far too
weak to matter:

| quantity | value |
|---|---|
| holds a surpassing card, having just declined | 32.2% |
| holds one, not having declined | 41.6% |
| likelihood ratio on the **event** | 0.775 |
| **per card** ruled against by a decline | 34.51% vs 35.88% → **0.962** |

The event-level signal and the per-card signal differ by 25× in effect, and only
the second is what a per-card weight table can use. Weighting the deal by it
moves recall against the true hand from 37.19% to 37.18%, and the mean weight
landing on cards actually held (0.9763) is indistinguishable from cards not held
(0.9779). The code is kept, defaulted off, behind `beliefEnabled`.

**Why the game resists it.** In a trick-taking game the strong inference is the
*void*: a player who cannot follow suit is forced to reveal it. Arcs has no
follow-suit obligation — Copy and Pivot are always legal whatever you hold — so
declining to Surpass is a choice, not a confession, and the bots decline 65% of
the time they could have surpassed. The public record carries much less about
hands than the shape of the game suggests.

**Four wrong constants on the way to one right one.** The parameter went 0.42
(guessed), 0.649 (measured, but the wrong conditional), 0.866 (0.649 spread as
`odds^(1/k)`, assuming an independence structure the data does not have), and
finally 0.962 (measured per card, which is what the model consumes). Each was
wrong in a way the previous metric could not see, and the one that finally
exposed the confusion was the simplest possible diagnostic: *is the mean weight
on cards actually held higher than on cards not held?* It was 0.9984 — the model
pointed nowhere.

That diagnostic also caught a real bug the fancier metrics had been silently
absorbing. Weight rows were sized by `v.actionDeck.length`, which is 20 at 2–3
players, but they are indexed by **card id**, which spans the full 28-card deck
because removing the 1s and 7s does not renumber the rest. Every id above 19 read
`undefined`, poisoning the arithmetic to `NaN` — and since `NaN <= 0` is false,
the guard for "no weight left" did not fire, the roulette loop's comparisons all
failed, and the sampler fell through to its last-index default. It had quietly
stopped being random at all.

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

Every number below is from the **paired** harness, playing the setup-card deck —
draw one of the four cards for your player count, take positions on it at
random. Earlier editions of this table are not comparable and should not be
cited.

2 players:

| matchup | games | deals | win % | paired difference | separated |
|---|---|---|---|---|---|
| `greedy` vs `random+` | 120 | 60 | **100.0** / 0.0 | +100.0 ±0.0 | yes |
| `mcts` vs `greedy` | 120 | 60 | **71.7** / 28.3 | +43.3 ±17.0 | yes |
| `random+` vs `random` | 2000 | 1000 | **60.5** / 39.6 | +20.9 ±4.2 | yes |
| `greedy` vs `mc` | 120 | 60 | 59.2 / 40.8 | +18.3 ±18.3 | **no** |

3 players:

| field | games | deals | win % | paired (`greedy` vs `mc`) |
|---|---|---|---|---|
| `greedy`, `mc`, `random` | 180 | 30 | **65.0** / 35.0 / 0.0 | +30.0 ±12.3, separated |

    mcts  >  greedy  >  mc  >  random+  >  random

`greedy` vs `mc` is the one rung not settled head-to-head, though `greedy` takes
the 3-player field comfortably.

### The opening is a variable, and holding it still flatters results

The same matchup, the same agents, three different ways of choosing the opening:

| openings | `greedy` vs `mc` | separated |
|---|---|---|
| 6 fixed rotations | +31.7 ±15.1 | yes |
| 4 setup cards (as played) | +18.3 ±18.3 | no |
| ~3000 free draws | +13.3 ±17.1 | no |

Nothing about either agent changed between those rows. The original six
rotations gave every player a home cluster with the same two planets of it, and
whatever edge `greedy` had was substantially an edge **on those six boards**.
Widening the pool of openings halves it and takes the interval across zero.

The 3-player field kept its separation throughout (+30.0 ±12.3 on the deck), the
same pattern as every other shake-up in this file: **the multiplayer result is
the durable one**, probably because a third player's presence swamps whatever
small edge a particular opening confers.

This is why `SetupMode` has two settings rather than one. `deck` is the game as
played and is the default. `draw` treats the opening as a nuisance variable, and
is what to use when the question is "is this agent better" rather than "who wins
Arcs".

### `mcts` beats `greedy`, and this one survived the harness fix

Of everything in this file, the claim most likely to have been an artefact was
the newest and boldest one: that `mcts` had finally pulled clear of `greedy`. It
was measured on the biased instrument, so it had to be re-run before it could be
believed.

It holds through every change to how it is measured. 120 paired games over 60
deals on the setup deck:

| | win % | mean Power | paired difference |
|---|---|---|---|
| `mcts` | **71.7** | 25.6 | **+43.3 ±17.0** |
| `greedy` | 28.3 | 22.0 | |

32 deals to `mcts`, 6 to `greedy`, 22 split. The reading has gone 71.7 (broken
harness) → 65.0 (fixed harness, six rotations) → 69.2 (free draws) → 71.7 (setup
deck), excluding 50% every time since the harness was fixed. It is the one
result in this file that has survived a bias fix, a map transcription, a
complete Court and three different schemes for choosing the opening.

`mcts` also holds its lead on mean Power (23.5 vs 22.3), which had been the one
stable difference running the *other* way for most of this project's history.

The five-reading history of this matchup — 50/50, 35/65, 55/45, 41.7/58.3,
28.3/71.7 — is best read now as four measurements from a broken instrument plus
one that happened to point the right way. It is not evidence that successive data
corrections moved the ladder, because the instrument was moving too. What can be
said is that on a working instrument, at the current rules, `mcts` is ahead.

The likely reason is that a complete Court is the part of Arcs that rewards
lookahead: card abilities are conditional, sequenced and interacting — a Prelude
that places ships changes what a Battle can do later the same turn, securing a
Vox card opens a decision mid-turn — and a one-step evaluation prices those at
whatever the position looks like immediately afterwards. That is still a
hypothesis. Testing it needs an abilities-off switch the engine does not have.

### How much did the map correction actually move? Unknown.

This section used to claim the map transcription "moved the ladder more than any
agent change has", on the strength of `greedy` vs `mc` swinging 22.5 points.
**That claim is withdrawn.** Both the before and after numbers came off the
biased harness, and re-measuring the same matchup properly puts it back where it
started. The swing was the confound moving, not the map.

What survives is the mechanism, which is a statement about the game rather than
about a measurement. The invented map spread the types near-evenly; the printed
map has **Weapon ×4 and Relic/Psionic ×3**, so the two cheapest ambitions to
contest — Keeper and Empath — sit on the *scarce* resources, while Weapon worlds,
which score no ambition at all, are the common ones. An evaluation that prices
all five resources alike is systematically overvaluing a third of the board. That
remains a good reason to expect gains from weight tuning.

Whether the correction was worth ladder points is now simply unmeasured. Doing it
properly means re-running both map versions on the fixed harness, which is cheap
and has not been done.

The dice correction earlier in this file rests on the same compromised
instrument and deserves the same caveat.

### The map was missing two edges, and this time it was measured first

The 18 planets are a **ring**, not six separate rows of three. The rulebook says
a planet "is adjacent to one or both neighbouring planets" and never says the
neighbour has to be in the same cluster; the engine assumed it did, so all six
cluster boundaries were walls. On the printed board only **four** of them are —
the other two are ordinary thin borders, and ships cross them without routing
through a gate. The joins are 2.3–3.1 and 5.3–6.1.

Reported by someone reading the physical board, then confirmed from the setup
cards, which draw the same map with no planet art on top of it. Median-combining
all 12 cards erases their labels and out-of-play shading; scanning circles round
the centre then sorts the 18 radial borders into three clean tiers — 12 at
5.0–6.8px, two at 10.0 and 11.1px, four at 20.2–24.5px. The wide four also
*wander* 3–7× as much in angle, which is what "thick, irregular" means; the two
middle ones are dead straight. Full method in
[DATA-GAPS.md](DATA-GAPS.md#the-border-transcription).

**Did it move the ladder?** Barely, and not detectably. Same seed, same 30
deals, one data change:

| planet ring | `greedy` win % (3p field) | `greedy` vs `mc` |
|---|---|---|
| closed (before) | 69.4 ±6.7 | +38.9 ±10.4 |
| open (after) | 65.6 ±6.9 | +31.1 ±11.7 |

Both moved the same way — the extra route slightly narrows `greedy`'s edge — but
each reading sits inside the other's interval, so the direction is suggestive
and nothing more. The reason is visible in the setup: a join only exists if both
its clusters are in play, which happens for **0.52 of the 2 joins per game at 3
players**, 0.97 at 2 and 1.00 at 4. Half the correction is out of play most of
the time. (Those rates come off the *reconstructed* setup deck, so they are a
property of four invented cards, not of the box.)

This is the third time an environment change has been checked for ladder impact
rather than assumed to have none — which is the habit the harness bug was
supposed to teach. Unlike the dice and the earlier map transcription, this one
was measured on the fixed harness, before any claim was written down.

### `random+` *is* an improvement — four null readings were the harness

This file said four times that "never end a turn with actions unspent, never burn
a card to seize" was not measurably better than uniform random: 52.5/47.5, then
62.5/37.5, then 52.5/47.5, all inside a ±15.5 interval on 40 games.

On the fixed harness, 2000 paired games over 1000 deals:

| | win % | paired difference |
|---|---|---|
| `random+` | **59.0** | **+18.0 ±4.2** |
| `random` | 41.0 | |

335 deals to `random+`, 155 to `random`, 510 split. That is not close, and it is
not a small effect — 18 points is larger than anything else separating two bots
here.

Two things went wrong before, and they compound. The batches were 40 games, far
too few for a ±15 interval to say anything. And the harness carried a systematic
seat/setup confound worth about 14 points, in an unknown direction per matchup.
A real 18-point effect is entirely capable of hiding under that, and it did.

The lesson is not "we needed more games", though we did. It is that a null result
from an instrument you have not validated is not a null result. The instrument
here could not tell an agent apart from a copy of itself.

### Flat Monte-Carlo: second in the field, unresolved head-to-head

`mc` samples worlds and plays each candidate action out several times, which is
strictly more information than greedy's single settled lookahead. It has never
finished ahead of `greedy` in a multiplayer field, and heads-up it is
**unresolved** on every opening scheme wider than the original six:

| | games | win % | paired difference | separated |
|---|---|---|---|---|
| `greedy` vs `mc`, 2 players | 120 | 59.2 / 40.8 | +18.3 ±18.3 | no |
| `greedy`, `mc`, `random`, 3 players | 180 | 65.0 / 35.0 / 0.0 | +30.0 ±12.3 | yes |

The structural reason `mc` should be weaker still holds: it has no tree, so it
cannot see its own follow-up pips. An Arcs turn is a *sequence* of 1–4 dependent
actions (build a starport, then build a ship at it; move in, then battle), and
evaluating the first action of that sequence against a random continuation
prices the setup at close to nothing.

The head-to-head number has now moved three times for reasons that had nothing
to do with either agent — the seat/setup confound, then free-draw openings, then
the setup deck. The field result held still through all of it. Stated as a rule
of thumb: **in this game the field result is the durable one.**

### Greedy builds more cities; the Power gap did not survive

Across every earlier version of the rules `mcts` finished with lower mean Power
than `greedy` while staying level on wins, and that looked like a structural
consequence of valuing rollouts on final standing rather than on Power: the
search has no reason to prefer a position that scores 40 and loses over one that
scores 24 and wins, and greedy's evaluation is Power-shaped and cannot express
that difference.

On the paired harness the gap is **gone or slightly reversed**: `mcts` scores 23.5
to `greedy`'s 22.3 while winning 65.0%. What survives, and has survived every
version of every measurement here, is the **city count**: `greedy` builds 4.4 to
`mcts`'s 3.8, and has built more in every single run.

So the mechanism was wrong even though it fit four consecutive datasets. It was
never a consequence of valuing rollouts on standing; it was some mixture of the
invented economy and the biased harness. The narrower claim — that greedy
over-builds cities because cities are what its Power-shaped evaluation can see —
is the one that held, and it held because it is a statement about the evaluation
rather than about a win rate.

That is a general pattern worth naming. Across this project the claims that
survived scrutiny were the ones about *mechanism* — greedy cannot see cascades,
greedy cannot see hand quality, greedy over-builds cities — and the ones that
collapsed were the ones about *ordering*. Mechanism claims are checkable against
a single game; ordering claims need an instrument, and the instrument was broken.

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
- **Batch size — still the binding constraint.** Pairing removed the bias but not
  the spread, so the 120-game rows still carry ±15 to ±17. The engine runs ~1000
  random games a second and `greedy` about 35, so 2000-game batches are minutes
  for the cheap agents and the reason not to run them is `mcts` at 3s a game.
  Making the search cheaper is therefore also a *measurement* improvement.
- **An abilities-off switch.** The claim that a complete Court is what let `mcts`
  separate is untested, because there is no way to run the same batch with the
  abilities disabled. A variant flag that keeps the card data but drops
  `power`/`vox` dispatch would turn that hypothesis into a measurement.
- **Re-measuring the data corrections.** The dice and map corrections were each
  credited with moving the ladder, on the compromised instrument. Both are cheap
  to re-run properly, and until that happens their effect on playing strength is
  unknown rather than established.
- **A stronger inference model.** The decline signal is a dead end for the reason
  given above — Arcs has no follow-suit obligation, so there is no void to infer.
  If anything is to be gained here it will come from a learned model over the
  whole public record rather than one hand-specified feature. The metric harness
  (`tools/belief-eval.ts`) is in place to judge it.
- **Learned value function.** The state is large but regular (24 systems ×
  pieces, 5 ambitions, hand shape); a TD(λ) afterstate net is the natural
  follow-up now that the Court is complete.
