import {
  applyActionMut,
  getPending,
  makeVariant,
  mulberry32,
  newGame,
  resolveChanceMut,
  type Action,
  type BattleState,
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

/**
 * Drive the game forward with a trivial policy until the round advances past
 * the one in progress. Used to observe end-of-round effects.
 */
export function settleRound(f: Fixture, limit = 200): void {
  const startRounds = f.s.stats.rounds;
  for (let i = 0; i < limit; i++) {
    if (f.s.stats.rounds > startRounds) return;
    const node = getPending(f.s, f.v);
    if (node.kind === 'over') return;
    if (node.kind === 'chance') {
      resolveChanceMut(f.s, f.v, f.rng);
      continue;
    }
    // Prefer the action that moves the round along fastest.
    const prefer =
      node.actions.find((a) => a.t === 'endTurn') ??
      node.actions.find((a) => a.t === 'passInitiative') ??
      node.actions[0];
    applyActionMut(f.s, f.v, prefer);
  }
  throw new Error('round did not end within the step limit');
}

/** The same, but run to the end of the chapter so scoring happens. */
export function settleToChapterEnd(f: Fixture, limit = 400): void {
  const startChapters = f.s.stats.chapters;
  for (let i = 0; i < limit; i++) {
    if (f.s.stats.chapters > startChapters) return;
    const node = getPending(f.s, f.v);
    if (node.kind === 'over') return;
    if (node.kind === 'chance') {
      resolveChanceMut(f.s, f.v, f.rng);
      continue;
    }
    const prefer =
      node.actions.find((a) => a.t === 'passInitiative') ??
      node.actions.find((a) => a.t === 'endTurn') ??
      node.actions[0];
    applyActionMut(f.s, f.v, prefer);
  }
  throw new Error('chapter did not end within the step limit');
}

/**
 * A battle mid-resolution, with the roll already made. Only the fields a test
 * cares about need naming; the rest default to "nothing rolled".
 */
export function battleState(partial: Partial<BattleState> & Pick<BattleState, 'system' | 'attacker' | 'defender'>): BattleState {
  return {
    rolled: { assault: [], skirmish: [], raid: [] },
    dice: { assault: 0, skirmish: 0, raid: 0 },
    selfHits: 0,
    intercept: 0,
    hits: 0,
    buildingHits: 0,
    keys: 0,
    interceptResolved: false,
    skirmishBlanks: 0,
    pendingReroll: 0,
    rerollDone: false,
    ...partial,
  };
}

/** Find the first action matching a predicate, or throw with context. */
export function find<T extends Action>(list: Action[], pred: (a: Action) => boolean): T {
  const hit = list.find(pred);
  if (!hit) throw new Error(`no matching action among: ${list.map((a) => a.t).join(', ')}`);
  return hit as T;
}
