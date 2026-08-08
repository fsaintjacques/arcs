/**
 * Ambitions, ambition markers and end-of-chapter scoring (rulebook p18-p19).
 */
import type {
  AmbitionId,
  AmbitionMarkerDef,
  GameState,
  PlayerState,
  ResourceType,
  VariantDef,
} from './types';
import { AMBITIONS } from './types';
import { courtCard } from './court';
import { cartelIcons } from './powers';
import { BONUS_FIRST_ONE_SLOT, BONUS_FIRST_BOTH_SLOTS, uncoveredBonuses } from './playerBoard';

/**
 * The 3 ambition markers. Blue (starting) sides are legible in the rulebook
 * (p3): 5/3, 3/2, 2/0.
 *
 * DATA-GAP: only the 2/0 marker's orange side (4/2) is sourced; the other two
 * reverses are extrapolated — see docs/DATA-GAPS.md §1.
 */
export const AMBITION_MARKERS: AmbitionMarkerDef[] = [
  { blue: { first: 5, second: 3 }, orange: { first: 9, second: 5 } },
  { blue: { first: 3, second: 2 }, orange: { first: 6, second: 4 } },
  { blue: { first: 2, second: 0 }, orange: { first: 4, second: 2 } },
];

/** Power a marker is currently worth, given whether it has been flipped. */
export function markerValue(
  markers: AmbitionMarkerDef[],
  index: number,
  flipped: boolean,
): { first: number; second: number } {
  const m = markers[index];
  return flipped ? m.orange : m.blue;
}

/**
 * "Take the highest-number ambition marker from the Available Markers section"
 * (p9) — highest by current first-place Power.
 */
export function highestAvailable(s: GameState, v: VariantDef): number | null {
  let best: number | null = null;
  let bestValue = -1;
  for (const i of s.availableMarkers) {
    const value = markerValue(v.ambitionMarkers, i, s.flipped[i]).first;
    if (value > bestValue) {
      bestValue = value;
      best = i;
    }
  }
  return best;
}

/**
 * "Flip over the ambition marker with the lowest Power that hasn't been
 * flipped yet to its side with more Power" (p19).
 */
export function flipLowestUnflipped(s: GameState, v: VariantDef): void {
  let target: number | null = null;
  let lowest = Infinity;
  for (let i = 0; i < v.ambitionMarkers.length; i++) {
    if (s.flipped[i]) continue;
    const value = v.ambitionMarkers[i].blue.first;
    if (value < lowest) {
      lowest = value;
      target = i;
    }
  }
  if (target !== null) s.flipped[target] = true;
}

// ---------------------------------------------------------------------------
// Counting ambitions
// ---------------------------------------------------------------------------

type CountedType = 'material' | 'fuel' | 'relic' | 'psionic';

/**
 * Resource + Guild-card icons of a type a player holds.
 *
 * `cartel` adds the resources sitting on a Cartel card this player holds: "you
 * add it to Tycoon but can't spend it" (p20). It is passed in rather than read
 * off `PlayerState` because the stockpile lives on the card, not the board.
 */
function icons(
  p: PlayerState,
  cartel: Partial<Record<ResourceType, number>> | undefined,
  ...types: CountedType[]
): number {
  let n = 0;
  for (const r of p.resources) if (r && (types as string[]).includes(r)) n++;
  for (const id of p.guildCards) {
    const suit = courtCard(id).suit;
    if (suit && (types as string[]).includes(suit)) n++;
  }
  for (const t of types) n += cartel?.[t] ?? 0;
  return n;
}

/** What a player has toward one ambition (p18). */
export function ambitionCount(
  p: PlayerState,
  ambition: AmbitionId,
  cartel?: Partial<Record<ResourceType, number>>,
): number {
  switch (ambition) {
    case 'tycoon':
      return icons(p, cartel, 'material', 'fuel');
    case 'tyrant':
      return p.captives.length;
    case 'warlord':
      return p.trophies.length;
    case 'keeper':
      return icons(p, cartel, 'relic');
    case 'empath':
      return icons(p, cartel, 'psionic');
  }
}

export interface AmbitionResult {
  ambition: AmbitionId;
  first: number;
  second: number;
  /** Counts per player, plus the 2-player phantom under key `phantom`. */
  counts: number[];
  phantom: number;
  firstPlace: number[];
  secondPlace: number[];
  awards: number[];
}

/**
 * Score one ambition box.
 *
 * Ties for first: all tied players take second place instead. Ties for second:
 * nobody places. You cannot place with a count of 0 ("Qualifying", p18).
 * Bonus city Power applies only to an outright first place.
 */
export function scoreAmbition(
  s: GameState,
  v: VariantDef,
  ambition: AmbitionId,
): AmbitionResult | null {
  const markers = s.declared[ambition];
  if (markers.length === 0) return null;

  let first = 0;
  let second = 0;
  for (const i of markers) {
    const value = markerValue(v.ambitionMarkers, i, s.flipped[i]);
    first += value.first;
    second += value.second;
  }

  const counts = s.playerStates.map((p, i) => ambitionCount(p, ambition, cartelIcons(s, i)));
  const phantom = s.phantom[ambition] ?? 0;
  const pool = [...counts, phantom];

  const top = Math.max(...pool);
  const awards = counts.map(() => 0);
  const firstPlace: number[] = [];
  const secondPlace: number[] = [];

  if (top > 0) {
    const topPlayers = counts.map((c, i) => (c === top ? i : -1)).filter((i) => i >= 0);
    const phantomTop = phantom === top;
    const tiedForFirst = topPlayers.length + (phantomTop ? 1 : 0) > 1;

    if (!tiedForFirst && topPlayers.length === 1) {
      // Outright first place.
      const winner = topPlayers[0];
      firstPlace.push(winner);
      awards[winner] += first + bonusCityPower(s.playerStates[winner]);

      // Second place goes to the next distinct positive count, unless tied.
      const rest = pool.filter((c) => c < top && c > 0);
      if (rest.length > 0) {
        const runnerUp = Math.max(...rest);
        const runners = counts.map((c, i) => (c === runnerUp ? i : -1)).filter((i) => i >= 0);
        const phantomRunner = phantom === runnerUp;
        if (runners.length + (phantomRunner ? 1 : 0) === 1 && runners.length === 1) {
          secondPlace.push(runners[0]);
          awards[runners[0]] += second;
        }
      }
    } else {
      // Everyone tied for first drops to second place; no bonus city Power.
      for (const i of topPlayers) {
        secondPlace.push(i);
        awards[i] += second;
      }
    }
  }

  return { ambition, first, second, counts, phantom, firstPlace, secondPlace, awards };
}

/** +2 / +5 for an outright first place, by uncovered city slots (p18). */
export function bonusCityPower(p: PlayerState): number {
  const { plusTwo, plusThree } = uncoveredBonuses(p);
  if (plusTwo && plusThree) return BONUS_FIRST_BOTH_SLOTS;
  if (plusTwo) return BONUS_FIRST_ONE_SLOT;
  return 0;
}

export interface ChapterScore {
  results: AmbitionResult[];
  gained: number[];
}

/** Score every declared ambition box. */
export function scoreChapter(s: GameState, v: VariantDef): ChapterScore {
  const results: AmbitionResult[] = [];
  const gained = s.playerStates.map(() => 0);
  for (const ambition of AMBITIONS) {
    const r = scoreAmbition(s, v, ambition);
    if (!r) continue;
    results.push(r);
    r.awards.forEach((a, i) => (gained[i] += a));
  }
  return { results, gained };
}
