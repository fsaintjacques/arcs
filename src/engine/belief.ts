/**
 * What the public record says about who holds what.
 *
 * `determinize()` has to guess the Rivals' hands. Dealing the unseen cards
 * uniformly throws away the richest signal in the game: **how people follow**.
 * A follower may Surpass only with the lead suit and a higher number (p10), so
 * a player who Copies or Pivots instead is evidence — not proof — that they
 * hold no such card. Over a chapter those declines accumulate.
 *
 * Arcs is unusually friendly to this. The hidden state is small: at 3 players
 * the deck is 20 cards, you hold 6, and the remaining 14 split 6/6/2. That is
 * tiny next to the belief states people wrestle with in poker, and it means a
 * cheap model captures most of what is knowable.
 *
 * The model is deliberately shallow — one signal, one parameter — because the
 * parameter is *measured* rather than guessed (see `tools/calibrate.ts`) and a
 * shallow model with a fitted constant beats a deep one with invented ones.
 */
import { actionCard, ALL_ACTION_CARDS } from './cards';
import type { GameState, Suit, VariantDef } from './types';

/**
 * Weight multiplier for a card a decline rules against.
 *
 * **Measured per card, not derived.** Over 60 greedy games: a card ruled
 * against by a decline is held 34.51% of the time, one not ruled against
 * 35.88% — a ratio of 0.962. Three earlier numbers were wrong on the way here
 * and each was wrong for an instructive reason:
 *
 *   0.42   guessed, and simply not measured;
 *   0.649  P(decline | could Surpass), which is the right quantity for the
 *          *event* "holds at least one surpassing card" but not for a card;
 *   0.866  0.649 spread over k candidates as `odds^(1/k)`, which assumed an
 *          independence structure the data does not have.
 *
 * The lesson is that the event-level signal (0.775) and the per-card signal
 * (0.962) differ by 25× in effect, and only the second is what a per-card
 * weight table can use. It is far too weak to matter — see `beliefEnabled`.
 *
 * Re-measure with `npx tsx tools/calibrate.ts` if the agents change materially.
 */
export const DECLINE_ODDS = 0.962;

/**
 * Whether `determinize` weights its deal by default. **Off.**
 *
 * Not an oversight — a measured negative result. At a per-card ratio of 0.962
 * the model shifts sampled hands by under a percentage point: recall against
 * the true hand is 37.18% weighted versus 37.19% uniform, and the mean weight
 * landing on cards actually held is 0.9763 against 0.9779 on cards not held.
 * It is indistinguishable from uniform, so it does not earn the work it costs.
 *
 * The reason is a property of Arcs rather than of the model. In a trick-taking
 * game the strong inference is the *void*: a player who cannot follow suit is
 * forced to reveal it. Arcs has no follow-suit obligation — Copy and Pivot are
 * always legal, whatever you hold — so declining to Surpass is a choice rather
 * than a confession, and the bots decline 65% of the time they could. The
 * public record simply carries much less about hands than the shape of the game
 * suggests.
 *
 * The machinery is kept because it is correct, tested and cheap to re-enable:
 * `declines` tracking, the calibration tool and `tools/belief-eval.ts` are the
 * reusable parts, and a learned inference model would drop straight into
 * `computeBelief` with the metric already in place to judge it.
 */
export const beliefEnabled = false;

/** Relative likelihood that each unseen card sits in each player's hand. */
export interface Belief {
  /** `weight[player][cardId]`, unnormalised. 1 is the uninformed prior. */
  weight: number[][];
}

/**
 * Build a belief from the public record: which follows declined to Surpass.
 *
 * Only the observer's Rivals get weights; their own hand is known.
 *
 * **Declines are deduplicated per (player, suit), keeping the lowest number.**
 * They are not independent observations: a player who declined over
 * "Aggression above 3" and later over "Aggression above 5" has told you one
 * thing twice, because the first constraint already covers every card the
 * second one does. Multiplying both compounds correlated evidence — measured
 * over 40 games it stacked up to four deep, driving the odds to 0.649⁴ ≈ 0.18
 * on cards the record only justifies discounting once. That over-sharpening
 * made the model's log-likelihood *worse* than uniform, which is how it was
 * caught; the fix is to keep the single strongest constraint per suit.
 */
export function computeBelief(s: GameState, v: VariantDef, observer: number): Belief {
  // Rows are indexed by *card id*, which spans the full 28-card deck even when
  // the variant only deals 20 of them: at 2-3 players the 1s and 7s are removed
  // but the remaining ids are not renumbered. Sizing these by
  // `v.actionDeck.length` leaves every id above 19 reading `undefined`, which
  // silently turns the sampler's arithmetic into NaN.
  const weight: number[][] = [];
  for (let p = 0; p < s.players; p++) {
    weight.push(new Array<number>(ALL_ACTION_CARDS.length).fill(1));
  }

  // Strongest (lowest-number) decline per player and suit.
  const strongest = new Map<string, { player: number; suit: Suit; number: number }>();
  for (const d of s.declines) {
    if (d.player === observer) continue;
    const key = `${d.player}:${d.suit}`;
    const prev = strongest.get(key);
    if (!prev || d.number < prev.number) strongest.set(key, d);
  }

  for (const d of strongest.values()) {
    const row = weight[d.player];
    for (const def of v.actionDeck) {
      if (def.suit !== d.suit || def.number <= d.number) continue;
      row[def.id] *= DECLINE_ODDS;
    }
  }
  return { weight };
}

/**
 * Deal `count` cards to `player` from `pool`, favouring cards the belief says
 * they are likely to hold. Removes what it takes from `pool`.
 *
 * Sequential weighted sampling without replacement. It is an approximation to
 * the true posterior — the exact joint would need the full 84k-world
 * enumeration — but it is unbiased in the marginals that matter and costs the
 * same as the uniform version it replaces.
 */
export function drawWeighted(
  pool: number[],
  count: number,
  weights: number[],
  rng: () => number,
): number[] {
  const taken: number[] = [];
  for (let k = 0; k < count && pool.length > 0; k++) {
    let total = 0;
    for (const c of pool) total += weights[c] ?? 1;
    // Every candidate ruled out — or a malformed weight table — falls back to
    // uniform. The explicit guard matters: `NaN <= 0` is false, so without it a
    // bad weight slips past into the roulette loop, where every comparison also
    // fails and the "last index" default silently makes the draw deterministic.
    let pick = 0;
    if (!(total > 0)) {
      pick = Math.floor(rng() * pool.length);
    } else {
      let r = rng() * total;
      for (let i = 0; i < pool.length; i++) {
        r -= weights[pool[i]];
        if (r <= 0) {
          pick = i;
          break;
        }
        pick = i;
      }
    }
    taken.push(pool[pick]);
    pool.splice(pick, 1);
  }
  return taken;
}

/**
 * Per-card probability that `player` holds it, for diagnostics and for anyone
 * who wants the marginals rather than a sampled world.
 *
 * Normalised so the row sums to the player's hand size, which is what makes it
 * a probability rather than a weight.
 */
export function marginals(
  belief: Belief,
  pool: number[],
  player: number,
  handSize: number,
): Map<number, number> {
  const out = new Map<number, number>();
  const w = belief.weight[player];
  let total = 0;
  for (const c of pool) total += w[c];
  if (total <= 0) {
    for (const c of pool) out.set(c, handSize / pool.length);
    return out;
  }
  for (const c of pool) out.set(c, Math.min(1, (w[c] / total) * handSize));
  return out;
}

/** The suits and numbers a decline rules against, for tests and the UI. */
export function declineTargets(v: VariantDef, suit: Suit, number: number): number[] {
  return v.actionDeck.filter((c) => c.suit === suit && c.number > number).map((c) => c.id);
}

export { actionCard };
