/**
 * The action deck (rulebook p3, p8-p10).
 *
 * 4 suits x numbers 1-7 = 28 cards. With 2-3 players the "1" and "7" cards are
 * returned to the box, leaving 20 cards numbered 2-6.
 */
import type { ActionCardDef, ActionKind, AmbitionId, Suit } from './types';
import { SUITS } from './types';

/** Pips are inversely related to the card number (p3, confirmed p8-p10). */
export const PIPS_BY_NUMBER: Record<number, number> = {
  1: 4,
  2: 4,
  3: 3,
  4: 3,
  5: 2,
  6: 2,
  7: 1,
};

/** The ambition a card can declare when led (p9). */
export const AMBITION_BY_NUMBER: Record<number, AmbitionId | 'any' | null> = {
  1: null,
  2: 'tycoon',
  3: 'tyrant',
  4: 'warlord',
  5: 'keeper',
  6: 'empath',
  7: 'any',
};

/** Which actions a suit's pips can buy (p8). */
export const SUIT_ACTIONS: Record<Suit, ActionKind[]> = {
  administration: ['tax', 'repair', 'influence'],
  aggression: ['battle', 'move', 'secure'],
  construction: ['build', 'repair'],
  mobilization: ['move', 'influence'],
};

/** All 28 cards, id = suitIndex * 7 + (number - 1). */
export const ALL_ACTION_CARDS: ActionCardDef[] = SUITS.flatMap((suit, si) =>
  [1, 2, 3, 4, 5, 6, 7].map((number) => ({
    id: si * 7 + (number - 1),
    suit,
    number,
    pips: PIPS_BY_NUMBER[number],
    ambition: AMBITION_BY_NUMBER[number],
  })),
);

export function actionCard(id: number): ActionCardDef {
  return ALL_ACTION_CARDS[id];
}

/** The deck for a player count: 2-6 always, plus 1s and 7s at 4 players (p4). */
export function actionDeckFor(players: number): ActionCardDef[] {
  return ALL_ACTION_CARDS.filter(
    (c) => (c.number >= 2 && c.number <= 6) || players === 4,
  );
}

export function cardName(id: number): string {
  const c = actionCard(id);
  return `${c.suit[0].toUpperCase()}${c.number}`;
}
