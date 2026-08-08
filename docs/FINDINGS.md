# Findings

Research notes from building the engine and the first bots. Headline numbers
are in the README; this keeps the evidence, the negative results, and the
methodology traps that cost real time.

Everything here describes **this engine's Arcs**, which is missing Guild card
powers and uses a reconstructed map (see [DATA-GAPS.md](DATA-GAPS.md)). Treat
the strategic conclusions as provisional until those land.

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

That is enough to reorder the ladder. Against the inferred dice, `mcts` and
`greedy` split 50/50 over 60 games. Against the real ones, `mcts` wins 65/35.
`random+` vs `random` moved the other way, from 65/35 to an even split.

The lesson is that aggregate statistics are not a substitute for the joint
distribution: a bot that decides *how many of which die to collect* is reading
exactly the structure that averages throw away.

## The ladder

All numbers below use the corrected dice.

2 players:

| matchup | games | win % | mean Power |
|---|---|---|---|
| `random+` vs `random` | 40 | 52.5 / 47.5 | 3.9 / 4.1 |
| `greedy` vs `random+` | 40 | 100.0 / 0.0 | 44.6 / 2.4 |
| `greedy` vs `mc` | 40 | 77.5 / 22.5 | 32.9 / 16.2 |
| `mcts` vs `greedy` | 60 | 65.0 / 35.0 | 23.2 / 24.3 |

3 and 4 players:

| field | games | win % |
|---|---|---|
| `greedy`, `mc`, `random` | 60 | 63.3 / 36.7 / 0.0 |
| `greedy`, `greedy`, `mc`, `random` | 48 | 50.0 / 35.4 / 14.6 / 0.0 |

### `random+` is not an improvement

"Never end a turn with actions unspent, never burn a card to seize" sounds
strictly better than uniform random, and over 40 games it is 52.5/47.5 — inside
the ±15.5 interval. Spending every pip on a randomly chosen action is not worth
much when the action is random. It remains useful as the rollout policy because
it keeps rollouts moving, but it is not evidence that the heuristic is sound.

### Flat Monte-Carlo loses to one-step greedy

`mc` samples worlds and plays each candidate action out several times, which is
strictly more information than greedy's single settled lookahead — and it loses
22.5/77.5 heads-up, and takes third of four behind two greedy seats.

The reason is structural: `mc` has no tree, so it cannot see its own follow-up
pips. An Arcs turn is a *sequence* of 1–4 dependent actions (build a starport,
then build a ship at it; move in, then battle), and evaluating the first action
of that sequence against a random continuation prices the setup at close to
nothing. More sampling does not beat a better-shaped one-step value here.

### MCTS wins by winning smaller

`mcts` takes `greedy` 65/35 over 60 games (±12.1) while finishing with *lower*
mean Power, 23.2 against 24.3. It also builds less: 3.9 cities to greedy's 4.5,
and fewer Tycoon and Keeper tokens.

That combination is the point rather than a puzzle. Rollouts are valued on final
standing, not on Power, so the search has no reason to prefer a position that
scores 40 and loses to one that scores 24 and wins. Greedy's evaluation is
Power-shaped and cannot express the difference.

It is worth stressing how much of this rests on the dice. The same matchup was
an even 50/50 against the inferred die faces. Search reads the joint
distribution of a dice pool — which faces co-occur, not just their averages —
so it was the agent most damaged by getting that distribution wrong, and the one
that gained most from fixing it.

Remaining levers, roughly in order of expected value:

1. **Iterations vs branching.** 300 iterations against a node with 100+ legal
   actions barely leaves the root. `narrow()` caps branching at 12, which helps,
   but the trim is uniform across kinds rather than informed.
2. **Rollout depth.** 30 decisions rarely reaches a chapter break, so most
   rollouts are valued by the same heuristic greedy uses.
3. **A greedy rollout policy** instead of `random+`, so the sampled
   continuations resemble the play the values are meant to describe.

## Open questions

- **Guild card powers.** The largest gap. Until the Court has real abilities,
  the value of Influence and Secure is understated, so bots almost certainly
  under-invest in the Court relative to real Arcs.
- **Ambition timing.** No bot yet reasons about *when* to declare. The zero
  marker makes a declared lead card trivially surpassable, so declaring costs
  the initiative — a tradeoff none of the current evaluations model.
- **Weight tuning.** `eval.ts` weights are hand-set. A CEM or paired-seed grid
  search over them is the obvious next step and is likely worth more than
  further search depth.
- **Learned value function.** The state is large but regular (24 systems ×
  pieces, 5 ambitions, hand shape); a TD(λ) afterstate net is the natural
  follow-up once the Court is complete.
