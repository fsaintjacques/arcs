/**
 * The Court deck: 25 Guild cards + 6 Vox cards (rulebook p3, p4 step H, p17).
 *
 * A Guild card has a suit matching one of the 5 resources — it adds to Tycoon /
 * Keeper / Empath exactly like a resource token, and Weapon cards add to no
 * ambition (p17) — plus a raid cost and rules text. Vox cards resolve
 * immediately when secured and are discarded.
 *
 * DATA-GAP: the printed cards' rules text is not in the rulebook, so the deck
 * ships mechanically vanilla: influencing, securing, raiding, Outrage discards
 * and ambition counts are all exact, but no card grants a special power. The
 * `whenSecured` hook exists so the real cards are a data edit — see
 * docs/DATA-GAPS.md §4.
 */
import type { CourtCardDef, ResourceType } from './types';

/**
 * Guild suit distribution over the 25 cards. Weapon cards are the most common
 * because they score no ambition, so they are the deck's "action" cards; the
 * four scoring suits are evenly spread.
 */
// DATA-GAP: invented distribution.
const GUILD_SUITS: ResourceType[] = [
  ...Array<ResourceType>(5).fill('material'),
  ...Array<ResourceType>(5).fill('fuel'),
  ...Array<ResourceType>(5).fill('weapon'),
  ...Array<ResourceType>(5).fill('relic'),
  ...Array<ResourceType>(5).fill('psionic'),
];

/** Raid costs cycle 1-2-3 so stealing cards has a real price gradient (p17). */
const GUILD_RAID_COSTS = [1, 2, 3];

export const GUILD_CARDS: CourtCardDef[] = GUILD_SUITS.map((suit, i) => ({
  id: i,
  name: `${suit[0].toUpperCase()}${suit.slice(1)} Guild ${Math.floor(i / 5) === i / 5 ? '' : ''}${(i % 5) + 1}`.trim(),
  kind: 'guild' as const,
  suit,
  raidCost: GUILD_RAID_COSTS[i % GUILD_RAID_COSTS.length],
}));

export const VOX_CARDS: CourtCardDef[] = Array.from({ length: 6 }, (_, i) => ({
  id: GUILD_CARDS.length + i,
  name: `Vox ${i + 1}`,
  kind: 'vox' as const,
  suit: null,
  raidCost: 0,
  discardOnSecure: true,
}));

export const COURT_DECK: CourtCardDef[] = [...GUILD_CARDS, ...VOX_CARDS];

export function courtCard(id: number): CourtCardDef {
  return COURT_DECK[id];
}

/** Court row width: 3 at 2 players, 4 at 3-4 players (p4 step H). */
export function courtRowSize(players: number): number {
  return players === 2 ? 3 : 4;
}
