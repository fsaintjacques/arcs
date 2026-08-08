/**
 * Battle dice (rulebook p14).
 *
 * The rulebook gives the five symbols and the resolution order but not the
 * face distributions.
 *
 * DATA-GAP: the tables below are reconstructed as the simplest distributions
 * satisfying every published constraint simultaneously — see
 * docs/DATA-GAPS.md §1. Edit them here and nothing else changes.
 */
import type { DieFace, DieType, RNG } from './types';

const face = (
  hits: number,
  selfHits = 0,
  buildingHits = 0,
  keys = 0,
  intercept = 0,
): DieFace => ({ hits, selfHits, buildingHits, keys, intercept });

/** "A single hit on three faces, blank on the other three." Confirmed. */
export const SKIRMISH_FACES: DieFace[] = [
  face(1),
  face(1),
  face(1),
  face(0),
  face(0),
  face(0),
];

/**
 * Constraints: >=1 hit on 5 of 6 faces; 2 hits on 2 of 6; a self-hit on 3 of 6;
 * intercept on 1 of 6.
 */
export const ASSAULT_FACES: DieFace[] = [
  face(1),
  face(1),
  face(1, 1),
  face(2, 1),
  face(2, 1),
  face(0, 0, 0, 0, 1),
];

/**
 * Constraints: keys on 3 of 6 faces; a building hit on 3 of 6; self-hits on
 * more faces than assault; never damages defending ships; carries an intercept.
 */
export const RAID_FACES: DieFace[] = [
  face(0, 1, 0, 1),
  face(0, 1, 0, 1),
  face(0, 1, 0, 2),
  face(0, 1, 1, 0),
  face(0, 0, 1, 0),
  face(0, 0, 1, 0, 1),
];

export const DIE_FACES: Record<DieType, DieFace[]> = {
  assault: ASSAULT_FACES,
  skirmish: SKIRMISH_FACES,
  raid: RAID_FACES,
};

/** Dice of each type in the box (p3) — the per-type collection cap (p14). */
export const DICE_PER_TYPE = 6;

export function rollDie(type: DieType, rng: RNG): DieFace {
  const faces = DIE_FACES[type];
  return faces[Math.floor(rng() * faces.length)];
}

export interface RollTotals {
  selfHits: number;
  intercept: number;
  hits: number;
  buildingHits: number;
  keys: number;
}

export function rollBattle(dice: Record<DieType, number>, rng: RNG): RollTotals {
  const totals: RollTotals = { selfHits: 0, intercept: 0, hits: 0, buildingHits: 0, keys: 0 };
  for (const type of ['assault', 'skirmish', 'raid'] as DieType[]) {
    for (let i = 0; i < dice[type]; i++) {
      const f = rollDie(type, rng);
      totals.selfHits += f.selfHits;
      totals.intercept += f.intercept;
      totals.hits += f.hits;
      totals.buildingHits += f.buildingHits;
      totals.keys += f.keys;
    }
  }
  return totals;
}

/** Expected values per die, for heuristic bots. */
export function expectedFace(type: DieType): RollTotals {
  const faces = DIE_FACES[type];
  const acc: RollTotals = { selfHits: 0, intercept: 0, hits: 0, buildingHits: 0, keys: 0 };
  for (const f of faces) {
    acc.selfHits += f.selfHits / faces.length;
    acc.intercept += f.intercept / faces.length;
    acc.hits += f.hits / faces.length;
    acc.buildingHits += f.buildingHits / faces.length;
    acc.keys += f.keys / faces.length;
  }
  return acc;
}
