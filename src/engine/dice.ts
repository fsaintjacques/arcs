/**
 * Battle dice (rulebook p14, faces from the official Arcs aid booklet p3).
 *
 * The aid booklet prints all six faces of each die, so these are exact. The
 * intercept symbol is a ring that may enclose other symbols: an assault face
 * shows a hit inside the ring (hit + intercept), a raid face shows two keys
 * inside it (2 keys + intercept), and a bare ring is intercept alone.
 */
import type { DieFace, DieType, RNG } from './types';

const face = (
  hits: number,
  selfHits = 0,
  buildingHits = 0,
  keys = 0,
  intercept = 0,
): DieFace => ({ hits, selfHits, buildingHits, keys, intercept });

/** Three faces with a single hit, three blank. Never hurts the attacker. */
export const SKIRMISH_FACES: DieFace[] = [
  face(1),
  face(1),
  face(1),
  face(0),
  face(0),
  face(0),
];

/**
 * 2 hits · 2 hits + self-hit · hit + intercept · hit + self-hit ·
 * hit + self-hit · blank.
 */
export const ASSAULT_FACES: DieFace[] = [
  face(2),
  face(2, 1),
  face(1, 0, 0, 0, 1),
  face(1, 1),
  face(1, 1),
  face(0),
];

/**
 * 2 keys + intercept · key + self-hit · building hit + key ·
 * building hit + self-hit · building hit + self-hit · intercept.
 *
 * Raid dice never damage defending ships, which is why the Raid Dice Limit
 * (p14) gates them on the defender having buildings to lose.
 */
export const RAID_FACES: DieFace[] = [
  face(0, 0, 0, 2, 1),
  face(0, 1, 0, 1),
  face(0, 0, 1, 1),
  face(0, 1, 1, 0),
  face(0, 1, 1, 0),
  face(0, 0, 0, 0, 1),
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
  /** Skirmish dice that came up blank — what Skirmishers may reroll. */
  skirmishBlanks: number;
}

export interface RollResult extends RollTotals {
  /** The face each die landed on, as indices into `DIE_FACES[type]`. */
  faces: Record<DieType, number[]>;
}

export function rollBattle(dice: Record<DieType, number>, rng: RNG): RollResult {
  const result: RollResult = {
    selfHits: 0,
    intercept: 0,
    hits: 0,
    buildingHits: 0,
    keys: 0,
    skirmishBlanks: 0,
    faces: { assault: [], skirmish: [], raid: [] },
  };
  for (const type of ['assault', 'skirmish', 'raid'] as DieType[]) {
    const all = DIE_FACES[type];
    for (let i = 0; i < dice[type]; i++) {
      const idx = Math.floor(rng() * all.length);
      const f = all[idx];
      result.faces[type].push(idx);
      result.selfHits += f.selfHits;
      result.intercept += f.intercept;
      result.hits += f.hits;
      result.buildingHits += f.buildingHits;
      result.keys += f.keys;
      if (type === 'skirmish' && f.hits === 0) result.skirmishBlanks++;
    }
  }
  return result;
}

/**
 * Reroll `count` blank skirmish dice, returning the hits gained, how many
 * came up blank again, and the new faces. Only blanks are ever rerolled — see
 * the ruling in game.ts — so a reroll can only ever help.
 */
export function rerollSkirmish(
  count: number,
  rng: RNG,
): { hits: number; blanks: number; faces: number[] } {
  let hits = 0;
  let blanks = 0;
  const faces: number[] = [];
  for (let i = 0; i < count; i++) {
    const idx = Math.floor(rng() * SKIRMISH_FACES.length);
    faces.push(idx);
    if (SKIRMISH_FACES[idx].hits > 0) hits += SKIRMISH_FACES[idx].hits;
    else blanks++;
  }
  return { hits, blanks, faces };
}

/** Expected values per die, for heuristic bots. */
export function expectedFace(type: DieType): RollTotals {
  const faces = DIE_FACES[type];
  const acc: RollTotals = {
    selfHits: 0,
    intercept: 0,
    hits: 0,
    buildingHits: 0,
    keys: 0,
    skirmishBlanks: 0,
  };
  for (const f of faces) {
    acc.selfHits += f.selfHits / faces.length;
    acc.intercept += f.intercept / faces.length;
    acc.hits += f.hits / faces.length;
    acc.buildingHits += f.buildingHits / faces.length;
    acc.keys += f.keys / faces.length;
  }
  return acc;
}
