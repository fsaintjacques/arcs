/**
 * Imperfect information: what a player may legally see, and how to sample a
 * consistent full state from it.
 *
 * Arcs hides two things (p22, "Private Information"): the action cards in
 * other players' hands, and cards played face down (Copy plays and cards
 * spent to seize the initiative). Deck order is hidden from everyone.
 *
 * `observe()` blanks those out; `determinize()` deals them back at random in a
 * way consistent with everything the observer has seen, which is what
 * determinized search (ISMCTS) needs.
 */
import { cloneState } from './board';
import { shuffle } from './rng';
import type { GameState, RNG, VariantDef } from './types';

/** A legal view of the game for one player. */
export interface Observation {
  /** The observer. */
  player: number;
  /** State with hidden information replaced by counts. */
  state: GameState;
  /** How many cards each player holds (own hand is exact in `state`). */
  handSizes: number[];
  /** Cards the observer knows are face down in front of each player. */
  faceDownCounts: number[];
}

/** Every action card that exists in this variant. */
function allCards(v: VariantDef): number[] {
  return v.actionDeck.map((c) => c.id);
}

/**
 * Cards whose location the observer knows: their own hand, everything already
 * discarded face up (played face up this round), and their own face-down plays.
 */
function knownCards(s: GameState, player: number): number[] {
  const known = [...s.playerStates[player].hand];
  for (const c of s.round.played) {
    if (!c.faceDown || c.player === player) known.push(c.card);
  }
  return known;
}

export function observe(s: GameState, v: VariantDef, player: number): Observation {
  const view = cloneState(s);

  for (let p = 0; p < s.players; p++) {
    if (p === player) continue;
    view.playerStates[p].hand = [];
  }
  // Deck and discard order are unknown to everyone.
  view.actionDeck = [];
  view.actionDiscard = [];
  view.courtDeck = [];
  // Face-down plays by others are hidden.
  view.round.played = view.round.played.map((c) =>
    c.faceDown && c.player !== player ? { ...c, card: -1 } : c,
  );

  return {
    player,
    state: view,
    handSizes: s.playerStates.map((p) => p.hand.length),
    faceDownCounts: s.playerStates.map(
      (_, p) => s.round.played.filter((c) => c.faceDown && c.player === p).length,
    ),
  };
}

/**
 * Sample a full state consistent with an observation: deal the unseen action
 * cards back into the other hands, the face-down plays, the deck and the
 * discard, and reshuffle the Court deck.
 *
 * The result is a legal world, not *the* world — that is exactly what
 * determinized search wants.
 */
export function determinize(obs: Observation, v: VariantDef, rng: RNG): GameState {
  const s = cloneState(obs.state);
  const known = new Set(knownCards(s, obs.player));
  const pool = shuffle(
    allCards(v).filter((c) => !known.has(c)),
    rng,
  );

  for (let p = 0; p < s.players; p++) {
    if (p === obs.player) continue;
    s.playerStates[p].hand = pool.splice(0, obs.handSizes[p]);
  }
  s.round.played = s.round.played.map((c) =>
    c.card === -1 ? { ...c, card: pool.pop() ?? 0 } : c,
  );

  // Whatever is left is the deck and discard; the split does not matter because
  // a chapter deal reshuffles both together.
  s.actionDeck = [];
  s.actionDiscard = pool;

  // The Court deck's order is unknown; rebuild it from the cards not in play.
  const seen = new Set<number>([
    ...s.court.map((c) => c.card),
    ...s.courtDiscard,
    ...s.playerStates.flatMap((p) => p.guildCards),
  ]);
  s.courtDeck = shuffle(
    v.courtDeck.map((c) => c.id).filter((id) => !seen.has(id)),
    rng,
  );

  return s;
}

/** Convenience: observe then immediately re-sample a world. */
export function sampleWorld(
  s: GameState,
  v: VariantDef,
  player: number,
  rng: RNG,
): GameState {
  return determinize(observe(s, v, player), v, rng);
}
