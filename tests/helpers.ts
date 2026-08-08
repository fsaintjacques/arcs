import {
  applyActionMut,
  getPending,
  makeVariant,
  mulberry32,
  newGame,
  resolveChanceMut,
  type Action,
  type GameState,
  type VariantDef,
} from '../src/engine';

export interface Fixture {
  v: VariantDef;
  s: GameState;
  rng: () => number;
}

/** A game dealt and sitting at the first lead decision. */
export function startGame(players = 3, seed = 1, setupIndex = 0): Fixture {
  const v = makeVariant(players, setupIndex);
  const rng = mulberry32(seed);
  const s = newGame(v, rng, setupIndex);
  resolveChanceMut(s, v, rng); // deal
  if (s.phase === 'mulligan') applyActionMut(s, v, { t: 'mulligan', take: false });
  return { v, s, rng };
}

export function actions(f: Fixture): Action[] {
  const node = getPending(f.s, f.v);
  if (node.kind !== 'decision') throw new Error(`expected decision, got ${node.kind}`);
  return node.actions;
}

export function actor(f: Fixture): number {
  const node = getPending(f.s, f.v);
  if (node.kind !== 'decision') throw new Error(`expected decision, got ${node.kind}`);
  return node.player;
}

export function apply(f: Fixture, a: Action): void {
  applyActionMut(f.s, f.v, a);
}

/** Resolve any chance nodes standing in the way. */
export function settle(f: Fixture, limit = 64): void {
  for (let i = 0; i < limit; i++) {
    const node = getPending(f.s, f.v);
    if (node.kind !== 'chance') return;
    resolveChanceMut(f.s, f.v, f.rng);
  }
}

/** Force a specific card into a player's hand, for deterministic rule tests. */
export function giveCard(f: Fixture, player: number, card: number): void {
  const hand = f.s.playerStates[player].hand;
  if (!hand.includes(card)) hand.push(card);
}

/**
 * Replace a hand outright. Cards taken away go to the action discard, so the
 * deck stays conserved and later chapters still deal full hands.
 */
export function setHand(f: Fixture, player: number, cards: number[]): void {
  const hand = f.s.playerStates[player].hand;
  for (const card of hand) if (!cards.includes(card)) f.s.actionDiscard.push(card);
  f.s.playerStates[player].hand = [...cards];
}

/** Card id from suit index (0 admin, 1 aggression, 2 construction, 3 mobilization). */
export function cardId(suitIndex: number, number: number): number {
  return suitIndex * 7 + (number - 1);
}

export const ADMIN = 0;
export const AGGRESSION = 1;
export const CONSTRUCTION = 2;
export const MOBILIZATION = 3;

/** Find the first action matching a predicate, or throw with context. */
export function find<T extends Action>(list: Action[], pred: (a: Action) => boolean): T {
  const hit = list.find(pred);
  if (!hit) throw new Error(`no matching action among: ${list.map((a) => a.t).join(', ')}`);
  return hit as T;
}
