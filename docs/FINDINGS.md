# Findings

Research notes from building the engine and the first bots. Headline numbers
are in the README; this keeps the evidence, the negative results, and the
methodology traps that cost real time.

Everything here describes **this engine's Arcs**. All 31 Court cards, the whole
map — planet types, building slots and every border round the planet ring — and
all 12 setup cards are transcribed from the printed game. Two small component
values are still reconstructed (see [DATA-GAPS.md](DATA-GAPS.md)), and the bots
are weak in absolute terms, so treat the strategic conclusions as provisional.

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

### Thinking time measured inside saturated workers is contention, not cost

The gauntlet's budget gate originally timed the candidate's `choose()` calls
wherever they ran. On the worker-pool runner that meant nine threads of
search agents saturating ten cores — plus, that day, a second experiment
running alongside — and `mcts` was reported at **910 ms per decision**
against its real, in-process cost of about **10 ms**. The strength number
from the same batch was fine (decisions are seeded; contention changes when
they finish, not what they choose), but a budget gate fed by that clock
would have failed every candidate the moment the machine got busy.

The fix: the parallel gauntlet now measures thinking time in a short serial
sample after the batch (`timingGames`, default one paired block). Thinking
time is a property of the agent; wall-clock inside a saturated pool is a
property of the pool. The worker pool itself is healthy — 18 identical
`mcts` games ran 3.53 s/game serially and 0.70 s/game across nine workers, a
5× speedup — which is exactly why the 91× reading had to be the clock and
not the work.

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

Every number below is from the **paired** harness at `--seed 1`, playing the
**printed** setup deck — draw one of the four cards for your player count, take
positions from the initiative marker. Earlier editions of this table are not
comparable and should not be cited.

2 players:

| matchup | games | deals | win % | paired difference | separated |
|---|---|---|---|---|---|
| `greedy` vs `random+` | 120 | 60 | **100.0** / 0.0 | +100.0 ±0.0 | yes |
| `mcts` vs `greedy` | 120 | 60 | **70.0** / 30.0 | +40.0 ±14.1 | yes |
| `random+` vs `random` | 2000 | 1000 | **60.0** / 40.0 | +20.0 ±4.2 | yes |
| `greedy` vs `mc` | 120 | 60 | 51.7 / 48.3 | +3.3 ±15.4 | **no** |

3 players:

| field | games | deals | win % | paired (`greedy` vs `mc`) |
|---|---|---|---|---|
| `greedy`, `mc`, `random` | 180 | 30 | **57.2** / 42.8 / 0.0 | +14.4 ±13.5, separated |

    mcts  >  greedy  >  mc  >  random+  >  random

`greedy` vs `mc` is the one rung not settled head-to-head, and on the printed
deck it is a dead heat — though `greedy` still takes the 3-player field.

### The opening is a variable, and holding it still flatters results

The same matchup, the same agents, four different ways of choosing the opening:

| openings | `greedy` vs `mc` | separated |
|---|---|---|
| 6 fixed rotations | +31.7 ±15.1 | yes |
| 4 reconstructed cards | +18.3 ±18.3 | no |
| ~3000 free draws | +13.3 ±17.1 | no |
| **the printed 12** | **+3.3 ±15.4** | no |

Nothing about either agent changed between those rows. The original six
rotations gave every player a home cluster with the same two planets of it, and
whatever edge `greedy` had was substantially an edge **on those six boards**.
Widening the pool of openings halved it and took the interval across zero.

Then the printed cards arrived and halved it again, twice over. That last row is
the sharpest version of the lesson, because the four cards it replaced were not
arbitrary — they were *scored for balance* by this project, on criteria this
project chose (equal building capacity, distinct resource types, no crowding).
Openings picked to be fair to a metric still flattered the agent that reads that
metric. The real cards are less tidy — 3 of the 12 hand one player an extra
building slot — and on them the two agents are a dead heat.

The 3-player field kept its separation throughout (+14.4 ±13.5 on the printed
deck, down from +30.0 ±12.3 but still clear of zero), the same pattern as every
other shake-up in this file: **the multiplayer result is the durable one**,
probably because a third player's presence swamps whatever small edge a
particular opening confers.

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
deals on the printed setup deck:

| | win % | mean Power | paired difference |
|---|---|---|---|
| `mcts` | **70.0** | 28.2 | **+40.0 ±14.1** |
| `greedy` | 30.0 | 24.9 | |

26 deals to `mcts`, 2 to `greedy`, 32 split. The reading has gone 71.7 (broken
harness) → 65.0 (fixed harness, six rotations) → 69.2 (free draws) → 71.7
(reconstructed deck) → 70.0 (printed deck), excluding 50% every time since the
harness was fixed. It is the one result in this file that has survived a bias
fix, a map transcription, a complete Court, a corrected planet ring and four
different schemes for choosing the opening — including the one that flattened
`greedy` vs `mc` to nothing.

`mcts` also holds its lead on mean Power (28.2 vs 24.9), which had been the one
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
and nothing more. Part of the reason is that a join only exists if both its
clusters are in play. On the printed setup deck that is **0.97 of the 2 joins
per game at 3 players**, 1.30 at 2 and 1.25 at 4 — so on average a third of the
correction is covered by an out-of-play marker.

(Those two rows were measured against the *reconstructed* setup deck, which was
still in place at the time. They have not been re-run on the printed one. The
comparison is internally consistent — both rows share a deck — so the conclusion
"no detectable movement" stands, but the absolute win rates in it are stale.)

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
| `greedy` vs `mc`, 2 players | 120 | 51.7 / 48.3 | +3.3 ±15.4 | no |
| `greedy`, `mc`, `random`, 3 players | 180 | 57.2 / 42.8 / 0.0 | +14.4 ±13.5 | yes |

The structural reason `mc` should be weaker still holds: it has no tree, so it
cannot see its own follow-up pips. An Arcs turn is a *sequence* of 1–4 dependent
actions (build a starport, then build a ship at it; move in, then battle), and
evaluating the first action of that sequence against a random continuation
prices the setup at close to nothing.

The head-to-head number has now moved four times for reasons that had nothing to
do with either agent — the seat/setup confound, then free-draw openings, then a
reconstructed setup deck, then the printed one — and it has moved the same way
every time, from +31.7 to +3.3. The field result held its separation through all
of it. Stated as a rule of thumb: **in this game the field result is the durable
one.**

### Greedy builds more cities; the Power gap did not survive

Across every earlier version of the rules `mcts` finished with lower mean Power
than `greedy` while staying level on wins, and that looked like a structural
consequence of valuing rollouts on final standing rather than on Power: the
search has no reason to prefer a position that scores 40 and loses over one that
scores 24 and wins, and greedy's evaluation is Power-shaped and cannot express
that difference.

On the paired harness the gap is **gone or slightly reversed**: `mcts` scores
28.2 to `greedy`'s 24.9 while winning 70.0%. What survives, and has survived
every version of every measurement here, is the **city count**: `greedy` builds
4.4 to `mcts`'s 4.2, and has built more in every single run.

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

## The hand-quality term arrived backwards, and one line turned it around

`eval.ts` now carries the terms the section above asked for: total pips in
hand (`handPips`), a per-card bonus when the card's suit can act on the
actual position (`handActionable` — an Aggression card with no reachable
fight is just a number), holding the highest live card of a suit
(`handHighCard`), a `declarableLead` term for strictly leading an undeclared
ambition while a marker remains to declare it, and per-type resource pricing
(`resourceValue`: Relic and Psionic at 0.85, Material and Fuel at 0.65,
Weapon at 0.5, replacing the flat 0.7 the map's type counts never justified).

First measurement, `greedy` with the new terms against the frozen
`anchor-greedy-v0`, 240 paired games: **−2.5 ± 12.6**. No better than the
weights it was supposed to improve on, and pointing the wrong way.

The diagnosis is worth keeping. Leading a card *converts* its pips into the
turn's actions — the pips leave the hand and reappear as `turn.pipsLeft`. A
term that prices the stock in hand and credits nothing for the flow it
becomes taught greedy to protect its inventory: it started leading its
*low*-pip cards to keep the high-pip ones in hand, forfeiting the very
actions the pips exist to buy. One line — crediting `turn.pipsLeft` at the
same rate as pips still in hand — flipped the identical 240-game row to
**+33.8 ± 10.8, separated**, a 55.8% absolute win rate at a three-player
table, and replicated at a fresh seed (**+41.3 ± 14.4**, 60.8%).

The general lesson joins the cascade-settling one: evaluation terms are
altitude-dependent. A 1-ply search reads a stock term as "never spend the
stock" unless the evaluation also sees what spending it buys. Any future
term that prices a resource a bot must *use* (cards, resources, agents)
needs its conversion credited, or shallow search will hoard it.

Two honest footnotes. First, `mcts` with the same weights read
**−3.8 ± 14.2** against `anchor-mcts300-v0` — flat. Its evaluation is
applied thirty random decisions downstream of the tree at rollout leaves,
which dilutes any leaf-level improvement; the truncated-search agent planned
next applies the evaluation at the leaf directly and is where these terms
should start paying inside search. Pricing pending Union attachments as
hand-card claims (the same stock-without-flow shape again) nudged the row to
**+6.3 ± 13.2** — point-positive, still not separated, promoted on the
no-regression criterion.

Second, re-running the 40-game instrumentation says the never-taken trio was
only half blind spot. Farseers' recycle moved from 0 of 998 offers to **2 of
102** — priced now, and taken when the hand is bad enough, which is the
correct rate for an ability that also costs its psionic icon and can hand a
rival the Empath lead. Union attachment stayed at **0 of 93**, but for a
legible reason: the claim maxes out at a 4-pip card against a full
Guild-card price under the hand-set weights, so whether it should ever fire
is now a threshold the tuner owns, not a payoff the evaluation cannot see.
The mulligan and the recycle both turn on what the hand is worth, which is
what the tests assert.

## Exact dice made the 1-ply bot weaker

The battle dice are printed, so a battle's outcome distribution is a small
exact convolution (`src/agents/dicemath.ts`, memoized over the 343 possible
pools) — the lever an earlier section said nobody pulled. Wiring it into
`greedy` so a battle is valued as the true expectation instead of the mean
of three sampled rolls produced, on the same deals as the hand-quality rows:

| battle valuation | seed 11 | seed 42 |
|---|---|---|
| 3 sampled rolls | +33.8 ± 10.8 | +41.3 ± 14.4 |
| exact expectation | +27.5 ± 12.6 | +27.5 ± 14.0 |

Both readings still beat the anchor, but the exact version runs 6–14 points
behind the sampler it was supposed to improve on, in the same direction at
both seeds. The mechanism: a battle node offers ~17 dice splits, and an
argmax over noisy 3-sample estimates systematically selects the estimates
that got lucky — it plays battles priced near their optimistic tail. The
evaluation *underprices* fighting (trophies and keys are worth more than
their weights say — the under-fighting theme recurs across FINDINGS), so
that optimism was a subsidy pointing the right way, and removing the noise
removed the subsidy.

**This claim did not survive a bigger sample.** The Rust port measured the two
valuations against each other directly, 2400 paired games, and read
+1.8 ± 3.9 — level. See
[the Rust calibration](#exact-dice-did-not-make-the-1-ply-bot-weaker) below;
the two rows above differ with a ±16.6 interval on the difference, so the
6–14 points were never separated from zero.

`greedy` therefore keeps sampling by default (`battles: 'sample'`), with
`'exact'` as an option; the honest fix is a better-priced evaluation, not a
worse estimator, and the tuner now exists to look for it. The convolution
itself is unaffected and is what the candidate generator and the truncated
search need — those want E[hits] and E[keys] *rankings*, where noise has no
compensating virtue.

Also in this change: search node keys moved from `JSON.stringify` to a
canonical `encodeAction` (`src/engine/encode.ts`). Measured on real action
lists it is only ~1.1× faster — V8's stringify is quick — so the case for it
is not speed but stability: keys no longer depend on literal key order, an
exhaustive switch breaks the build when an Action variant is added, and the
encodings distinguish cases stringify only distinguishes by accident
(Farseers' `cards: []` vs no cards at all). Provably strength-neutral: both
keyings are injective over full-game action sweeps (asserted in tests), so
the search tree is identical.

## The tuner works; the transfer doesn't, yet

First CEM run over the 25 free evaluation weights (`tools/tune-cem.ts`: 30
generations, population 24, elite 6, 96 paired games per member against the
M2-default greedy, everything seeded): elite fitness climbed to +20 to +45
points of win share, and the elite mean is checked in as
`src/agents/weights/tuned1.ts` (`greedy-t1` / `mcts-t1`).

The weight movements read like judgments, not noise. The scarcity story got
amplified rather than merely confirmed — Relic 0.85 → 1.23, Psionic → 0.99,
Weapon → 0.38 — control rose 0.5 → 0.81, hand pips 0.15 → 0.26, and the
starport, hand-priced at 1.4, was cut to 0.96. Held out to fresh seeds the
tuned greedy beats `anchor-greedy-v0` by **+45.0 ± 13.9** (the M2 weights
read +33.8 on the same deals) and the M2-default greedy by +18.1 ± 6.5 and
+5.3 ± 6.4 over two 960-game batches — real, if lumpy across seeds.

The transfer to `mcts` is where the promotion died: +11.3 ± 11.9 and
+2.5 ± 10.5 against the frozen anchor, −6.3 ± 13.2 against the M2-weight
mcts — nothing separated. The rule was set before the run (a separated mcts
transfer, or no promotion), so `defaultWeights` stay as they are and no new
anchor is frozen.

That failure is the third data point in one straight line: hand terms moved
greedy +34 and mcts +6; exact dice moved greedy −6 and mcts not at all;
tuned weights move greedy +12-ish and mcts +7-ish, none of it separated.
Evaluation quality keeps paying at 1 ply and keeps drowning by 30 random
rollout decisions. The planned truncated-search agent — which values leaves
with the evaluation directly — is no longer just an optimization; it is the
only way any of the evaluation work of the last three sections can reach
the strongest agent. Re-run the tuning transfer the day it exists.

## Informed trimming: the first separated search-tier gain

`narrow(12)` was the ranked #1 lost-strength source: at nodes offering a
hundred actions it kept the first twelve in enumeration order. The
replacement (`src/agents/candidates.ts`) keeps narrow's skeleton —
deterministic, output ⊆ input, every kind stays visible — and re-orders each
kind so the round-robin's early picks are the ones worth a search budget.

Getting the ordering right was measured, not guessed, and the first guess
lost. A new coverage instrument replays seeded games and asks, at every wide
node: can a 12-action trim still reach the best 1-ply value a full scan
finds? Blind `narrow` scores **73.0%**. A hand-tuned positional heuristic
("prefer contact, big fleets toward rivals") scored **62.5%** — *worse than
blind*, because the enumeration order narrow inherits (ships ascending)
accidentally matches what the evaluation actually wants: small detachments
that claim empty systems without giving up control at home.

The fix came from asking what a bare move can even change in the
evaluation: nothing but **control**. So the move ordering now computes the
exact control swing per move — the same strict-majority arithmetic as
`controlOf`, applied to both endpoints, rivals included — and uses the
positional heuristic only to break ties. Coverage: **98.3%**. Battle splits
are ranked by exact dice expectations (E[hits] − 1.2·E[selfHits], keys when
there is something to raid), Farseers subsets by pips discarded, Pressgang
multisets by scarcity prices.

Strength, 240 paired games per row: `mcts-c` beats the *same-budget* `mcts`
— identical everything but the trim — by **+15.0 ± 12.7, separated**, and
the anchor by **+22.5 ± 11.6** (replicated **+16.3 ± 13.3** at a second
seed), at ~10 ms/decision. After three eval improvements that paid at 1 ply
and drowned in rollouts, the first change *inside the search* is also the
first separated gain at the search tier. Frozen as `anchor-mcts-c-v1`.

Two durable lessons. A trim heuristic must be measured against the
evaluation it serves — intuition about "interesting" actions lost to blind
enumeration order. And coverage must be scored against best *value*, not
best *action*: ties are everywhere (most moves swing nothing), and an
identity metric charges the trim for returning a different member of the
same optimum.

## The Rust harness reproduces the ledger, and disagrees about one thing

The Rust port (`rust/`, R-series milestones) has reached the point where it
can be checked against this file rather than against unit tests. Parity is
**statistical only** — clean SplitMix64 seeding, not a replication of
`mulberry32` — so a seed does not transfer and no two rows compare game for
game. What compares is a *matchup*: play the same table in Rust, at equal or
larger sample, and ask whether the reading lands inside the TypeScript row's
interval.

Nothing was tuned to make a number match. That would have destroyed the only
thing the exercise is for.

Sixteen comparisons against eight [gauntlet](GAUNTLET.md) rows — each row read
back at its own sample size and again at 4–10× it. **Fourteen land inside the
TypeScript row's interval**, and the two that do not are the same Rust
measurement.

| matchup (3p, candidate vs two anchor copies) | TS row | Rust, same N | Rust, 4–10× N |
|---|---|---|---|
| `greedy` vs `anchor-greedy-v0` | +33.8±10.8 (s11) | +25.0±14.9 | +37.6±4.2 |
| " | +41.3±14.4 (s42) | +48.8±12.9 | " |
| `greedy battles:'exact'` vs `anchor-greedy-v0` | +27.5±12.6 (s11) | +30.0±13.5 | **+42.4±4.2** |
| " | +27.5±14.0 (s42) | +40.0±11.7 | " |
| `mcts` vs `anchor-mcts300-v0` | +6.3±13.2 (s11) | +18.8±13.9 | +1.9±6.1 |
| `mcts-c` vs `mcts` | +15.0±12.7 (s11) | +18.8±15.2 | +16.2±6.2 |
| `mcts-c` vs `anchor-mcts300-v0` | +22.5±11.6 (s11) | +17.5±12.4 | +23.1±6.7 |
| " | +16.3±13.3 (s42) | +27.5±13.6 | " |

Every same-N reading is inside. The bolded 2400-game `battles:'exact'` reading
sits 0.9 and 2.3 points above the top of the two intervals it is checked
against — and it is the row this section is named for.

### Exact dice did not make the 1-ply bot weaker

"Exact dice made the 1-ply bot weaker" reported `battles: 'exact'` running
6–14 points behind the sampler at both seeds. But TS never measured that
head-to-head; it compared two 240-game rows against a common anchor and read
the gap between them. Rust can afford the direct comparison, and at 2400
paired games `greedy battles:'exact'` against `greedy` reads **+1.8 ± 3.9 —
not separated**. At ten times the sample, on the same deals, the sampler's
alleged edge is a dead heat pointing very slightly the other way.

The likeliest cause is not a port bug but the arithmetic of the original
claim. Two 240-game readings carrying ±12.6 and ±10.8 differ with a 95%
interval of ±16.6 on the difference; a 6.3-point gap between them is not
evidence of a 6.3-point effect, and "same direction at both seeds" is two
observations. The proposed mechanism — an argmax over noisy 3-sample estimates
plays the optimistic tail, and since the evaluation underprices fighting that
optimism was a subsidy pointing the right way — is a good story and may be
true at some magnitude. What is not supported is the magnitude. (The R4 dice
convolution is asserted equal to the TS one to 1e-12, so both engines compute
the same exact expectation; this is a claim about sample size, not about
arithmetic.)

`battles: 'sample'` stays the default, for a different reason than before: it
is the cheaper estimator and now reads level rather than worse, so nothing
measured argues for changing it. The claim that exact is *weaker* should be
read as withdrawn pending a four-figure replication.

### The 2-player ladder is stale, not divergent

The ladder table earlier in this file warns that "earlier editions of this
table are not comparable and should not be cited". The port makes that
concrete. Same protocol, seed 1, printed deck:

| matchup | this file | Rust |
|---|---|---|
| `greedy` vs `random+` (2p) | +100.0±0.0 | +100.0±0.0 |
| `random+` vs `random` (2p, 2000 games) | +18.0±4.2 | +13.5±4.2 |
| `mcts` vs `greedy` (2p) | +40.0±14.1 | +15.4±8.9 (480 games) |
| `greedy` vs `mc` (2p) | +3.3±15.4 | +31.7±11.6 (240 games) |
| `greedy` in the `greedy`/`mc`/`random` field (3p) | +14.4±13.5 | +45.0±10.9 |

The `random+` row is not a disagreement: two independent estimates each
carrying ±4.2 differ by 4.5 against a ±5.9 interval on the difference.

The other three are, and they share one cause. Those rows predate the M2
evaluation, which moved `greedy` **+33.8** against the frozen anchor while
moving `mcts` only **+6.3** — so every reading that puts the evaluation tier
against the search tier has moved since, in the direction Rust shows. Re-run
the 2-player `mcts` vs `greedy` row in Rust **with the weights it was measured
under** — `anchor-mcts300-v0` against `anchor-greedy-v0`, 480 paired games:

| | win % | mean Power | cities | paired difference |
|---|---|---|---|---|
| `anchor-mcts300-v0` | 66.0 | 27.0 | 4.2 | **+32.1 ±8.4** |
| `anchor-greedy-v0` | 34.0 | 24.7 | 4.4 | |

That is inside the TS row's interval, and it reproduces both of the row's
*secondary* statistics unprompted: this file records `mcts` ahead on mean
Power (28.2 to 24.9) and `greedy` building more cities (4.4 to 4.2) "in every
single run". A port that had the rules subtly wrong would not land three
independent numbers at once. `mc` and the 3-player field have the same shape —
both favour the evaluation tier by ~30 points more than the stale row — and
the same explanation is the leading hypothesis, though no frozen `mc` anchor
exists to prove it the same way.

What this does *not* explain away, and stays on the open list: R5 fixed a real
`determinize` bug (TS worlds were short every `revealed` card), which
legitimately changes every search agent's behaviour, so the port's search
agents were never expected to be move-for-move identical to TS. The evidence
that the change is small is the search-vs-search column above — `mcts-c` vs
`mcts`, the purest search-tier ablation in the ledger, reproduces at +16.2
against TS's +15.0.

### Throughput: the reason for the port

Same machine (10 cores), 3-player tables, both engines measured rather than
quoted. TS via `npx tsx src/sim/cli.ts --workers 1`; Rust via
`cargo test -p arcs-sim --release --test bench -- --ignored`.

| table | TS games/s | Rust games/s | speedup | Rust, 9 workers | pool speedup |
|---|---|---|---|---|---|
| `random` ×3 | 1200 | 17477 | **14.6×** | 93027 | 5.3× |
| `greedy` ×3 | 17.2 | 117 | **6.8×** | 828 | 7.1× |
| `mcts` ×3 | 0.34 | 1.5 | **4.4×** | 4.9 | 3.3× |
| `mcts2` ×3 | — | 0.6 | — | 2.7 | 4.1× |

Per-decision thinking time, sampled serially by the gauntlet as the budget
gate requires: `greedy` 0.02–0.05 ms against the TS ledger's 0.2–1.3, `mcts`
2.3–2.6 ms against 9.6–11.4.

The pool speedups (3.3–7.1× on nine workers) are the same shape the TS pool
showed, and for the same reason: a batch of games is embarrassingly parallel,
and what caps it is memory bandwidth and core sharing rather than anything in
the harness. The column that matters for the lab is the serial one. Every
2400-game gauntlet row in this section cost under a minute, and the 960-game
search rows cost minutes rather than hours — "batch size is the binding
constraint" is no longer true below the search tier, and is merely painful
above it.

## Open questions

- **Ambition timing, the initiative half.** The `declarableLead` term now
  prices *holding* a declarable lead, but no bot yet reasons about the cost
  side: the zero marker makes a declared lead card trivially surpassable, so
  declaring spends the initiative — a tradeoff none of the evaluations model.
  Two cards bear directly on it: Secret Order and Galactic Bards both suppress
  the zero marker, so a bot holding either should declare far more freely, and
  none of them notices.
- **Weight tuning.** All the `eval.ts` weights — including the new hand terms
  and the scarcity prices, which are hand-guessed — are unfitted. A CEM run
  over paired seeds is the next step, and the worker pool makes its
  population evaluations affordable.
- **Batch size — still the binding constraint, above the search tier.** Pairing
  removed the bias but not the spread, so the 120-game rows still carry ±15 to
  ±17. On the TypeScript engine that was the whole story: ~1200 random games a
  second, `greedy` at 17, `mcts` at 3s a game. The Rust engine takes those to
  17k / 117 / 1.5 per second serial, so four-figure batches of the evaluation
  tier are now seconds — but a 2400-game `mcts` row is still an hour of one
  core, and making the search cheaper is still a *measurement* improvement.
- **Re-run the 2-player ladder.** Its rows predate the M2 evaluation and the
  Rust calibration shows three of them have moved by ~30 points for that reason
  alone. They are cheap to redo now and nobody has.
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
