/**
 * Variant construction and game setup (rulebook p4-p5).
 *
 * DATA-GAP: the box's 12 setup cards are not in the rulebook, so setups are
 * generated to obey every stated setup rule — correct out-of-play cluster
 * count, one A/B/C system per player (two C systems at 2 players), A and B
 * always planets, nothing in an out-of-play cluster, no sharing — spread
 * symmetrically around the ring. See docs/DATA-GAPS.md §3.
 */
import { actionDeckFor } from './cards';
import { AMBITION_MARKERS } from './ambitions';
import { COURT_DECK, courtRowSize } from './court';
import { buildSystems, clusterOf, gateId, planetId, resolveAdjacency, CLUSTER_COUNT } from './map';
import { gainResource, newPlayerState, AGENTS } from './playerBoard';
import { shuffle } from './rng';
import type { AmbitionId, GameState, ResourceType, RNG, SystemState, VariantDef } from './types';
import { AMBITIONS, RESOURCE_TYPES } from './types';

/** Power that ends the game at a chapter break (p2). */
export const POWER_THRESHOLD: Record<number, number> = { 2: 33, 3: 30, 4: 27 };

export const MAX_CHAPTERS = 5;
export const HAND_SIZE = 6;

export interface SetupCard {
  name: string;
  outOfPlay: number[];
  /** Per player: the A, B and C systems (two Cs at 2 players). */
  starts: { a: number; b: number; c: number[] }[];
}

/**
 * Generate a setup for `players` at rotation `index`.
 *
 * 1 cluster is removed at 4 players, 2 at 2-3 players (p4 step J). Removed
 * clusters sit opposite each other so the surviving ring stays balanced.
 */
export function generateSetup(players: number, index: number): SetupCard {
  const rotation = ((index % CLUSTER_COUNT) + CLUSTER_COUNT) % CLUSTER_COUNT;
  const outOfPlay =
    players === 4
      ? [rotation]
      : [rotation, (rotation + CLUSTER_COUNT / 2) % CLUSTER_COUNT];

  const live: number[] = [];
  for (let c = 0; c < CLUSTER_COUNT; c++) {
    if (!outOfPlay.includes((c + rotation) % CLUSTER_COUNT)) live.push((c + rotation) % CLUSTER_COUNT);
  }
  // `live` is ordered around the ring starting from the rotation offset.
  const inPlay = live.filter((c) => !outOfPlay.includes(c));

  const starts = Array.from({ length: players }, (_, p) => {
    const home = inPlay[Math.round((p * inPlay.length) / players) % inPlay.length];
    const c = [gateId(home)];
    if (players === 2) {
      const second = inPlay[(inPlay.indexOf(home) + 1) % inPlay.length];
      c.push(gateId(second));
    }
    return { a: planetId(home, 0), b: planetId(home, 1), c };
  });

  return { name: `${players} Players - Ring ${rotation + 1}`, outOfPlay, starts };
}

export function makeVariant(players: number, setupIndex = 0): VariantDef {
  const setup = generateSetup(players, setupIndex);
  return {
    id: `arcs-${players}p-${setupIndex}`,
    name: `Arcs (${players} players, ${setup.name})`,
    players,
    systems: resolveAdjacency(buildSystems(), setup.outOfPlay),
    actionDeck: actionDeckFor(players),
    courtDeck: COURT_DECK,
    ambitionMarkers: AMBITION_MARKERS,
    courtRowSize: courtRowSize(players),
    powerThreshold: POWER_THRESHOLD[players],
    maxChapters: MAX_CHAPTERS,
    handSize: HAND_SIZE,
  };
}

export function setupCardFor(v: VariantDef, setupIndex: number): SetupCard {
  return generateSetup(v.players, setupIndex);
}

function newSystemState(players: number, outOfPlay: boolean): SystemState {
  return {
    fresh: Array(players).fill(0),
    damaged: Array(players).fill(0),
    buildings: [],
    outOfPlay,
  };
}

/**
 * Deal the opening position. The chapter's hands are dealt by the `deal`
 * chance node, so `newGame` leaves the game there.
 */
export function newGame(v: VariantDef, rng: RNG, setupIndex = 0): GameState {
  const setup = generateSetup(v.players, setupIndex);
  const dead = new Set(setup.outOfPlay);

  const systems = v.systems.map((s) => newSystemState(v.players, dead.has(s.cluster)));
  const playerStates = Array.from({ length: v.players }, () => newPlayerState());

  /** 5 tokens of each resource type in the general supply (p3). */
  const supply = {} as Record<ResourceType, number>;
  for (const r of RESOURCE_TYPES) supply[r] = 5;
  /** No Cartel card is in play at setup, so nothing sits on one. */
  const cartelZero = {} as Record<ResourceType, number>;
  for (const r of RESOURCE_TYPES) cartelZero[r] = 0;

  // p5 step N: 3 ships + city in A, 3 ships + starport in B, 2 ships in each C.
  setup.starts.forEach((start, p) => {
    const ps = playerStates[p];
    const place = (system: number, ships: number) => {
      systems[system].fresh[p] += ships;
      ps.shipsSupply -= ships;
    };
    place(start.a, 3);
    systems[start.a].buildings.push({
      player: p,
      kind: 'city',
      damaged: false,
      taxedThisTurn: false,
      builtThisTurn: false,
    });
    ps.citiesUsed++;
    place(start.b, 3);
    systems[start.b].buildings.push({
      player: p,
      kind: 'starport',
      damaged: false,
      taxedThisTurn: false,
      builtThisTurn: false,
    });
    ps.starportsSupply--;
    for (const c of start.c) place(c, 2);

    // p5 step O: gain the resources matching the A and B planet types.
    for (const system of [start.a, start.b]) {
      const type = v.systems[system].planetType;
      if (type && supply[type] > 0 && gainResource(ps, type)) supply[type]--;
    }
  });

  // p4 step K: at 2 players the covered planets' resources become a phantom rival.
  const phantom = {} as Record<AmbitionId, number>;
  for (const a of AMBITIONS) phantom[a] = 0;
  if (v.players === 2) {
    for (const cluster of setup.outOfPlay) {
      for (let p = 0; p < 3; p++) {
        const type = v.systems[planetId(cluster, p)].planetType;
        if (!type || supply[type] <= 0) continue;
        supply[type]--;
        if (type === 'material' || type === 'fuel') phantom.tycoon++;
        else if (type === 'weapon') phantom.warlord++;
        else if (type === 'relic') phantom.keeper++;
        else if (type === 'psionic') phantom.empath++;
      }
    }
  }

  const courtDeck = shuffle(
    v.courtDeck.map((c) => c.id),
    rng,
  );
  const court = Array.from({ length: v.courtRowSize }, () => ({
    card: courtDeck.pop()!,
    agents: Array(v.players).fill(0),
  }));

  const declared = {} as Record<AmbitionId, number[]>;
  for (const a of AMBITIONS) declared[a] = [];

  return {
    variant: v.id,
    players: v.players,
    chapter: 1,
    phase: 'deal',
    initiative: Math.floor(rng() * v.players),
    initiativeSeized: false,
    systems,
    playerStates,
    supply,
    cartel: cartelZero,
    court,
    courtDeck,
    courtDiscard: [],
    actionDeck: v.actionDeck.map((c) => c.id),
    actionDiscard: [],
    round: {
      turnIndex: 0,
      turnOrder: [],
      lead: null,
      leadNumber: 0,
      played: [],
      seizedBy: null,
      consecutivePasses: 0,
      ambitionDeclared: false,
    },
    turn: null,
    battle: null,
    move: null,
    declared,
    availableMarkers: v.ambitionMarkers.map((_, i) => i),
    flipped: v.ambitionMarkers.map(() => false),
    phantom,
    reinforcing: null,
    unions: [],
    pendingVox: null,
    peek: null,
    revealed: [],
    declines: [],
    stats: {
      rounds: 0,
      chapters: 0,
      battles: 0,
      cardsPlayed: 0,
      ambitionsDeclared: 0,
      seizes: 0,
    },
  };
}

/** Agents a player currently has sitting on Court cards. */
export function agentsOnBoard(s: GameState, player: number): number {
  return s.court.reduce((n, slot) => n + slot.agents[player], 0);
}

export const STARTING_AGENTS = AGENTS;
export { clusterOf };
