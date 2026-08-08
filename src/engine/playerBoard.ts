/**
 * The player board: resource slots, city slots and the rewards they uncover
 * (rulebook p7, p17, p18).
 *
 * Cities sit in slots along the top of the board and are placed leftmost-first;
 * as they leave the board they uncover resource slots and the two "to won
 * ambitions" Power bonuses. Cities returning to the board cover them again.
 *
 * DATA-GAP: the rulebook states the mechanism and the +2/+5 bonus values but
 * not the slot layout or the per-slot raid costs — see docs/DATA-GAPS.md §6.
 */
import type { PlayerState, ResourceType } from './types';
import { RESOURCE_TYPES } from './types';

export const CITY_SLOTS = 5;
export const SHIPS = 15;
export const STARPORTS = 5;
export const AGENTS = 10;

/** Bonus Power for an outright ambition win, by uncovered slots (p18). */
export const BONUS_FIRST_ONE_SLOT = 2;
export const BONUS_FIRST_BOTH_SLOTS = 5;

/** What each city slot uncovers when its city is built. */
export type CityReward = { kind: 'resource' } | { kind: 'plusTwo' } | { kind: 'plusThree' };

export const PLAYER_BOARD = {
  citySlots: [
    { kind: 'resource' },
    { kind: 'resource' },
    { kind: 'plusTwo' },
    { kind: 'resource' },
    { kind: 'plusThree' },
  ] as CityReward[],
  /** Slots open before any city is built (p5 step O gives 2 starting resources). */
  baseResourceSlots: 3,
  /** Keys an attacker must spend to steal the resource in each slot (p16-p17). */
  raidCosts: [1, 1, 2, 2, 3, 3],
};

/** Total resource slots once every city is built. */
export const MAX_RESOURCE_SLOTS =
  PLAYER_BOARD.baseResourceSlots +
  PLAYER_BOARD.citySlots.filter((c) => c.kind === 'resource').length;

/** How many resource slots are currently open for a player. */
export function openResourceSlots(p: PlayerState): number {
  let n = PLAYER_BOARD.baseResourceSlots;
  for (let i = 0; i < p.citiesUsed && i < PLAYER_BOARD.citySlots.length; i++) {
    if (PLAYER_BOARD.citySlots[i].kind === 'resource') n++;
  }
  return Math.min(n, MAX_RESOURCE_SLOTS);
}

export function uncoveredBonuses(p: PlayerState): { plusTwo: boolean; plusThree: boolean } {
  let plusTwo = false;
  let plusThree = false;
  for (let i = 0; i < p.citiesUsed && i < PLAYER_BOARD.citySlots.length; i++) {
    if (PLAYER_BOARD.citySlots[i].kind === 'plusTwo') plusTwo = true;
    if (PLAYER_BOARD.citySlots[i].kind === 'plusThree') plusThree = true;
  }
  return { plusTwo, plusThree };
}

/** Raid cost of a held resource, by slot index. */
export function raidCost(slot: number): number {
  return PLAYER_BOARD.raidCosts[Math.min(slot, PLAYER_BOARD.raidCosts.length - 1)];
}

export function newPlayerState(): PlayerState {
  const outrage = {} as Record<ResourceType, boolean>;
  for (const r of RESOURCE_TYPES) outrage[r] = false;
  return {
    power: 0,
    resources: Array(MAX_RESOURCE_SLOTS).fill(null),
    outrage,
    guildCards: [],
    trophies: [],
    captives: [],
    agentsSupply: AGENTS,
    shipsSupply: SHIPS,
    starportsSupply: STARPORTS,
    citiesUsed: 0,
    hand: [],
  };
}

/**
 * Put a resource in the leftmost empty open slot. Returns false when the
 * player has no room — "you must discard resources you cannot hold" (p17).
 */
export function gainResource(p: PlayerState, r: ResourceType): boolean {
  const open = openResourceSlots(p);
  for (let i = 0; i < open; i++) {
    if (p.resources[i] === null) {
      p.resources[i] = r;
      return true;
    }
  }
  return false;
}

/** Discard resources sitting in slots that a returning city has covered. */
export function compactResources(p: PlayerState): void {
  const open = openResourceSlots(p);
  const held = p.resources.filter((r): r is ResourceType => r !== null).slice(0, open);
  p.resources = Array(MAX_RESOURCE_SLOTS).fill(null);
  held.forEach((r, i) => (p.resources[i] = r));
}

export function heldResources(p: PlayerState): ResourceType[] {
  return p.resources.filter((r): r is ResourceType => r !== null);
}
