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

## The ladder so far

2 players:

| matchup | games | win % | mean Power |
|---|---|---|---|
| `random+` vs `random` | 40 | 65.0 / 35.0 | 4.9 / 3.5 |
| `greedy` vs `random+` | 40 | 100.0 / 0.0 | 43.0 / 2.4 |
| `greedy` vs `mc` | 40 | 75.0 / 25.0 | 34.7 / 19.5 |
| `greedy` vs `mcts` | 60 | 50.0 / 50.0 | 26.1 / 21.5 |

3 and 4 players:

| field | games | win % |
|---|---|---|
| `greedy`, `mc`, `random` | 60 | 60.0 / 40.0 / 0.0 |
| `greedy`, `greedy`, `mc`, `random` | 48 | 43.8 / 41.7 / 14.6 / 0.0 |

The two greedy seats in the 4-player run land at 43.8% and 41.7% — a useful
check that permuted seating is doing its job.

### Flat Monte-Carlo loses to one-step greedy

`mc` samples worlds and plays each candidate action out several times, which is
strictly more information than greedy's single settled lookahead — and it loses
75/25 heads-up, and takes third of four behind two greedy seats.

The reason is structural: `mc` has no tree, so it cannot see its own follow-up
pips. An Arcs turn is a *sequence* of 1–4 dependent actions (build a starport,
then build a ship at it; move in, then battle), and evaluating the first action
of that sequence against a random continuation prices the setup at close to
nothing. More sampling does not beat a better-shaped one-step value here.

### MCTS is not yet paying for its tree

The natural follow-up — same sampling, but with a tree over the turn's own
decisions — comes out at **exactly 50/50 over 60 games** (interval ±12.7), with
*lower* mean Power (21.5 vs 26.1) and noticeably fewer ambition tokens at game
end (Tycoon 1.6 vs 3.0, Keeper 0.6 vs 1.5). An earlier 24-game batch read 58/42
for `mcts`; that was inside the noise and did not survive the larger run.

Reading the token counts, `mcts` is not playing a subtler economic game — it is
building less of everything and drawing level anyway, which is what a search
whose rollouts are too shallow to see a chapter score looks like. Candidates,
roughly in order of expected value:

1. **Iterations vs branching.** 300 iterations against a node with 100+ legal
   actions barely leaves the root. `narrow()` caps branching at 12, which helps,
   but the trim is uniform across kinds rather than informed.
2. **Rollout depth.** 30 decisions rarely reaches a chapter break, so most
   rollouts are valued by the same heuristic greedy uses — the search is paying
   for sampling and getting greedy's opinion back.
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
