/**
 * Informed candidate generation — `narrow()` with eyes.
 *
 * `narrow` trims wide nodes deterministically, one action kind at a time,
 * but *within* a kind it keeps whatever the legality enumerator emitted
 * first — for `move` that is "first source, first neighbour, one ship",
 * which FINDINGS.md ranks as the search's single biggest loss. This keeps
 * narrow's skeleton (deterministic, output ⊆ input, every kind stays
 * visible) and replaces enumeration order with an informed ordering per
 * kind, so the round-robin's early picks are the actions worth searching.
 *
 * Contract: a pure filter. It never synthesises an action, so it cannot
 * change legality; it only decides which legal actions a budget-bound
 * search gets to see.
 */
import { actionCard, controlOf } from '../engine';
import type { Action, GameState, VariantDef } from '../engine';
import { expectedBattle } from './dicemath';
import type { Weights } from './eval';

export interface CandidateOpts {
  max: number;
  weights: Weights;
}

type Scored = { a: Action; score: number; i: number };

/** Stable ranking: score descending, enumeration order as the tiebreak. */
function ranked(list: Action[], score: (a: Action) => number): Action[] {
  return list
    .map((a, i): Scored => ({ a, score: score(a), i }))
    .sort((x, y) => y.score - x.score || x.i - y.i)
    .map((x) => x.a);
}

/** How much a destination is worth moving toward. Cheap and positional. */
function edgeScores(s: GameState, v: VariantDef, player: number, moves: Action[]): Map<number, number> {
  const scores = new Map<number, number>();
  for (const m of moves) {
    const to = (m as { to: number }).to;
    if (scores.has(to)) continue;
    const st = s.systems[to];
    let score = 0;
    let rivalShips = 0;
    let rivalBuildings = 0;
    for (let p = 0; p < s.players; p++) {
      if (p === player) continue;
      rivalShips += st.fresh[p] + st.damaged[p];
    }
    for (const b of st.buildings) if (b.player !== player) rivalBuildings++;
    if (rivalShips === 0 && controlOf(s, to) !== player) score += 2; // uncontested claim
    if (rivalBuildings > 0) score += 2; // something to raid or besiege
    if (rivalShips > 0) score += 1; // contact
    if (v.systems[to].kind === 'planet' && st.buildings.length < v.systems[to].buildingSlots) {
      score += 1; // room to build
    }
    scores.set(to, score);
  }
  return scores;
}

/** Who controls a system if `player`'s fresh count there were `myFresh`. */
function controlWith(fresh: number[], player: number, myFresh: number): number | null {
  let best = -1;
  let winner: number | null = null;
  let tied = false;
  for (let p = 0; p < fresh.length; p++) {
    const n = p === player ? myFresh : fresh[p];
    if (n > best) {
      best = n;
      winner = p;
      tied = false;
    } else if (n === best) {
      tied = true;
    }
  }
  if (best <= 0 || tied) return null;
  return winner;
}

/**
 * The exact control swing a move causes, in the 1-ply evaluation's terms.
 * Control is the only evaluation term a bare move changes (ships keep their
 * count, buildings stay put), so ordering moves by it *is* ordering them the
 * way a full applyAction scan would, at a fraction of the cost — the
 * coverage test measures exactly this agreement.
 */
function moveControlDelta(
  s: GameState,
  player: number,
  m: Extract<Action, { t: 'move' }>,
): number {
  const from = s.systems[m.from];
  const to = s.systems[m.to];
  let delta = 0;
  for (const [st, myAfter] of [
    [from, from.fresh[player] - m.ships],
    [to, to.fresh[player] + m.ships],
  ] as const) {
    const before = controlWith(st.fresh, player, st.fresh[player]);
    const after = controlWith(st.fresh, player, myAfter);
    if (before !== after) {
      if (before === player) delta -= 1;
      else if (before !== null) delta += 1; // a rival lost control
      if (after === player) delta += 1;
      else if (after !== null) delta -= 1; // a rival gained control
    }
  }
  return delta;
}

/**
 * Order one kind's actions so the best-looking come first. Unhandled kinds
 * keep enumeration order, which is exactly `narrow`'s behaviour.
 */
function orderKind(
  kind: string,
  list: Action[],
  s: GameState,
  v: VariantDef,
  player: number,
  w: Weights,
): Action[] {
  switch (kind) {
    case 'move': {
      const edges = edgeScores(s, v, player, list);
      return ranked(list, (a) => {
        const m = a as Extract<Action, { t: 'move' }>;
        // The exact control swing dominates; the positional edge score and a
        // smallest-detachment preference only break its ties.
        return moveControlDelta(s, player, m) * 100 + (edges.get(m.to) ?? 0) - m.ships * 0.01;
      });
    }
    case 'catapult': {
      const edges = edgeScores(s, v, player, list);
      return ranked(list, (a) => {
        const c = a as Extract<Action, { t: 'catapult' }>;
        return (edges.get(c.to) ?? 0) + c.ships * 0.01;
      });
    }
    case 'battle':
      return ranked(list, (a) => {
        const b = a as Extract<Action, { t: 'battle' }>;
        const e = expectedBattle(b.assault, b.skirmish, b.raid);
        const defenderBuildings = s.systems[b.system].buildings.filter(
          (bl) => bl.player === b.defender,
        ).length;
        // Hits win fights, self-hits lose them, keys only matter with
        // something to raid. A bigger pool of the right shape ranks higher.
        return e.hits + e.buildingHits * 0.6 - 1.2 * e.selfHits + (defenderBuildings > 0 ? e.keys * 0.8 : 0);
      });
    case 'cardPrelude':
      return ranked(list, (a) => {
        const c = a as Extract<Action, { t: 'cardPrelude' }>;
        if (!c.cards) return 0;
        // Farseers' recycle: prefer swapping many low-pip cards.
        let score = 0;
        for (const card of c.cards) score += 4 - actionCard(card).pips;
        return score * 0.1 + c.cards.length * 0.01;
      });
    case 'cardAction':
      return ranked(list, (a) => {
        const c = a as Extract<Action, { t: 'cardAction' }>;
        if (!c.gain) return 0;
        // Pressgang-style multisets: rank by what the resources are worth.
        let score = 0;
        for (const r of c.gain) score += w.resourceValue[r];
        return score;
      });
    default:
      return list;
  }
}

/**
 * Trim `actions` to at most `max`, keeping the ones worth a search budget.
 * Same guarantees as `narrow`: deterministic for a given state and list,
 * output ⊆ input, no kind ever disappears entirely.
 */
export function generateCandidates(
  s: GameState,
  v: VariantDef,
  player: number,
  actions: Action[],
  opts: CandidateOpts,
): Action[] {
  if (actions.length <= opts.max) return actions;

  const byKind = new Map<string, Action[]>();
  for (const a of actions) {
    const list = byKind.get(a.t) ?? [];
    list.push(a);
    byKind.set(a.t, list);
  }
  for (const [kind, list] of byKind) byKind.set(kind, orderKind(kind, list, s, v, player, opts.weights));

  // narrow()'s round-robin over kinds, now consuming informed orderings.
  const kinds = [...byKind.keys()];
  const out: Action[] = [];
  for (let round = 0; out.length < opts.max; round++) {
    let added = false;
    for (const k of kinds) {
      const list = byKind.get(k)!;
      if (round >= list.length) continue;
      out.push(list[round]);
      added = true;
      if (out.length >= opts.max) break;
    }
    if (!added) break;
  }
  return out;
}
