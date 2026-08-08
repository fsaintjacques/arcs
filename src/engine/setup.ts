/**
 * Variant construction and game setup (rulebook p4-p5).
 *
 * `SETUP_DECK` is the printed 12, transcribed from the cards. `generateSetup`
 * invents legal openings instead, for measurement batches that want more than
 * four boards per player count — see `SetupMode`.
 */
import { actionDeckFor } from './cards';
import { AMBITION_MARKERS } from './ambitions';
import { COURT_DECK, courtRowSize } from './court';
import { buildSystems, clusterOf, gateId, planetId, resolveAdjacency, CLUSTER_COUNT } from './map';
import { gainResource, newPlayerState, AGENTS } from './playerBoard';
import { mulberry32, pick, shuffle } from './rng';
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
 * The setup-card deck: the printed 12, 4 per player count (p4 step I).
 *
 * Transcribed from the card faces. Each card draws the map schematically — a
 * ring of 18 planet slices around 6 gate sectors — and prints a position label
 * in each system a player starts in. Reading them is a geometry problem rather
 * than an OCR one: the card's borders were measured once (see
 * docs/DATA-GAPS.md), which fixes every sector's angular span, and each label's
 * angle and radius then name the system it sits in. `tools/read-setups.mjs`
 * redraws the sector names over a card so the reading can be re-checked by eye.
 *
 * Systems are engine ids: `cluster * 4 + slot`, slot 0 the gate. So the printed
 * "3.1" is id 9 and the printed gate "5" is id 16.
 *
 * `starts[0]` is the card's 1A/1B/1C, `starts[1]` its 2A/2B/2C, and so on.
 * Which player takes which is not the card's business — see `newGame`.
 */
export const SETUP_DECK: Record<number, SetupCard[]> = {
  2: [
    // 2 Players - Mix Up 1
    { name: 'Mix Up 1', outOfPlay: [1, 4], starts: [
      { a: 14, b: 10, c: [0, 21] },
      { a: 23, b: 11, c: [2, 12] },
    ] },
    // 2 Players - Mix Up 2
    { name: 'Mix Up 2', outOfPlay: [0, 3], starts: [
      { a: 18, b: 5, c: [22, 8] },
      { a: 6, b: 21, c: [16, 11] },
    ] },
    // 2 Players - Homelands
    { name: 'Homelands', outOfPlay: [0, 3], starts: [
      { a: 17, b: 21, c: [19, 16] },
      { a: 11, b: 9, c: [5, 8] },
    ] },
    // 2 Players - Frontiers (marked "for experienced players")
    { name: 'Frontiers', outOfPlay: [0, 5], starts: [
      { a: 19, b: 15, c: [8, 11] },
      { a: 9, b: 17, c: [16, 13] },
    ] },
  ],
  3: [
    // 3 Players - Mix Up
    { name: 'Mix Up', outOfPlay: [0, 3], starts: [
      { a: 11, b: 18, c: [4] },
      { a: 19, b: 5, c: [8] },
      { a: 7, b: 9, c: [16] },
    ] },
    // 3 Players - Homelands
    { name: 'Homelands', outOfPlay: [4, 5], starts: [
      { a: 7, b: 10, c: [8] },
      { a: 3, b: 5, c: [4] },
      { a: 1, b: 15, c: [12] },
    ] },
    // 3 Players - Frontiers
    { name: 'Frontiers', outOfPlay: [1, 2], starts: [
      { a: 3, b: 15, c: [20] },
      { a: 19, b: 2, c: [16] },
      { a: 14, b: 21, c: [0] },
    ] },
    // 3 Players - Core Conflict (marked "for experienced players")
    { name: 'Core Conflict', outOfPlay: [2, 5], starts: [
      { a: 3, b: 6, c: [0] },
      { a: 7, b: 2, c: [4] },
      { a: 1, b: 5, c: [12] },
    ] },
  ],
  4: [
    // 4 Players - Mix Up 1
    { name: 'Mix Up 1', outOfPlay: [2], starts: [
      { a: 13, b: 23, c: [0] },
      { a: 15, b: 19, c: [20] },
      { a: 17, b: 3, c: [12] },
      { a: 21, b: 1, c: [16] },
    ] },
    // 4 Players - Mix Up 2
    { name: 'Mix Up 2', outOfPlay: [3], starts: [
      { a: 19, b: 9, c: [4] },
      { a: 11, b: 18, c: [0] },
      { a: 7, b: 3, c: [8] },
      { a: 1, b: 5, c: [16] },
    ] },
    // 4 Players - Mix Up 3
    { name: 'Mix Up 3', outOfPlay: [5], starts: [
      { a: 11, b: 18, c: [0] },
      { a: 1, b: 9, c: [4] },
      { a: 3, b: 15, c: [8] },
      { a: 13, b: 6, c: [16] },
    ] },
    // 4 Players - Frontiers
    { name: 'Frontiers', outOfPlay: [4], starts: [
      { a: 3, b: 10, c: [4] },
      { a: 7, b: 23, c: [8] },
      { a: 14, b: 5, c: [20] },
      { a: 1, b: 21, c: [12] },
    ] },
  ],
};

/**
 * How a game's opening is chosen.
 *
 * `deck` is the game as played: shuffle the cards for your player count, draw
 * one, and take positions on it (p4 step I). Four boards per count.
 *
 * `draw` invents a fresh legal opening per seed instead — thousands of boards.
 * It is not how the game is played, but few openings *flatter* results: on the
 * six fixed rotations this project used before, `greedy` beat `mc` by
 * +31.7 ±15.1, and on freely drawn openings the same matchup is +13.3 ±17.1 and
 * no longer separated. Use `draw` for large measurement batches where the
 * opening should be a nuisance variable rather than part of the game.
 */
export type SetupMode = 'deck' | 'draw';

/**
 * Draw one of the cards for this player count (p4 step I). `seed` picks it, so
 * the same seed always reproduces the same board — which is what lets
 * `makeVariant` and `newGame` derive it independently and still agree.
 *
 * Positions come back in the card's own order, 1 first. Handing them to players
 * is `newGame`'s job, because the rulebook ties it to the initiative marker.
 */
export function drawSetup(players: number, seed: number, mode: SetupMode = 'deck'): SetupCard {
  if (mode === 'draw') return generateSetup(players, seed);

  const rng = mulberry32((Math.abs(Math.trunc(seed)) * 2246822519 + 0xc0de) >>> 0);
  return pick(SETUP_DECK[players], rng);
}

/**
 * Draw a random legal setup for `players`, determined entirely by `seed`.
 *
 * `seed` was once an index into six fixed rotations, which meant a thousand-deal
 * batch only ever saw six opening positions — every measurement was averaging
 * over the same handful of boards. It now seeds a shuffle, so the same seed
 * still reproduces the same setup exactly (which `makeVariant` and `newGame`
 * both rely on to agree with each other) while distinct seeds give genuinely
 * different maps.
 *
 * Randomising openings is only safe *because* the batch runner pairs deals: a
 * lopsided draw is played from every seat before it counts, so it cancels
 * instead of becoming noise. Under the old unpaired runner this change would
 * have made results worse, not better.
 *
 * Every stated setup rule is obeyed (p4 step J, p5 steps N-O):
 *
 *   - 1 cluster out of play at 4 players, 2 at 2-3 (each takes its gate and all
 *     3 planets touching it);
 *   - one A, one B and one C system per player, two Cs at 2 players;
 *   - A and B are planets, since they take a city and a starport and their
 *     printed resource is gained at step O;
 *   - nothing starts in an out-of-play cluster, and no system is shared.
 */
export function generateSetup(players: number, seed: number): SetupCard {
  // A private stream, so drawing a setup never disturbs the game's RNG and the
  // two independent calls that build a game land on the same board.
  const rng = mulberry32((Math.abs(Math.trunc(seed)) * 2654435761 + 0x5e70) >>> 0);

  const removed = players === 4 ? 1 : 2;
  const order = shuffle(
    Array.from({ length: CLUSTER_COUNT }, (_, c) => c),
    rng,
  );
  const outOfPlay = order.slice(0, removed).sort((a, b) => a - b);
  const live = order.slice(removed);

  const claimed = new Set<number>();

  /**
   * Draw an unclaimed system, preferring the given clusters but never returning
   * one already taken. Preference has to be able to fail: a player's B planet
   * may spread into a neighbouring cluster and take the planet a later player
   * would have wanted, so falling back to "anywhere live" is what keeps the
   * no-sharing rule true rather than nearly true.
   */
  const take = (prefer: number[], kind: 'gate' | 'planet'): number => {
    const of = (clusters: number[]) =>
      clusters.flatMap((c) => (kind === 'gate' ? [gateId(c)] : [0, 1, 2].map((i) => planetId(c, i))));
    const free = of(prefer).filter((s) => !claimed.has(s));
    const anywhere = of(live).filter((s) => !claimed.has(s));
    const from = free.length > 0 ? free : anywhere;
    // The board always has room: 12-15 live planets and 4-5 live gates against
    // at most 8 planets and 4 gates needed.
    const chosen = pick(shuffle(from, rng), rng);
    claimed.add(chosen);
    return chosen;
  };

  const starts = Array.from({ length: players }, (_, p) => {
    // Each player homes in a distinct live cluster.
    const home = live[p % live.length];
    const neighbours = [
      (home + 1) % CLUSTER_COUNT,
      (home + CLUSTER_COUNT - 1) % CLUSTER_COUNT,
    ].filter((c) => live.includes(c));

    // A sits in the home cluster. B often sits in a neighbouring one: the
    // printed cards spread a player's two planets rather than stacking them,
    // which is what makes the opening moves matter.
    const a = take([home], 'planet');
    const b = take(neighbours.length > 0 && rng() < 0.5 ? neighbours : [home], 'planet');

    const c = [take([home], 'gate')];
    if (players === 2) c.push(take(neighbours, 'gate'));
    return { a, b, c };
  });

  return { name: `${players} Players - Draw ${seed}`, outOfPlay, starts };
}

/**
 * Build a variant. `setupIndex` seeds the opening — see `drawSetup`. The same
 * value always yields the same board, which is what lets `newGame` derive an
 * identical setup without the two having to share state; pass the same `mode`
 * to both or they will disagree about which clusters are out of play.
 */
export function makeVariant(players: number, setupIndex = 0, mode: SetupMode = 'deck'): VariantDef {
  const setup = drawSetup(players, setupIndex, mode);
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

export function setupCardFor(v: VariantDef, setupIndex: number, mode: SetupMode = 'deck'): SetupCard {
  return drawSetup(v.players, setupIndex, mode);
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
 *
 * **Which player takes which position on the card follows the initiative
 * marker**, not a separate draw: "The player with the initiative marker does
 * this: place 3 ships and 1 city in the system marked 1A… Going clockwise from
 * the player with initiative, the 2nd, 3rd, and 4th players set up in the same
 * way in the systems marked 2A-C, 3A-C, and 4A-C" (p5 step N). Initiative goes
 * to a random player at step B, so the assignment is random — but it is a
 * random *rotation* of the seats, not a random permutation of them, and the
 * player who opens on position 1 is always the player who moves first.
 */
export function newGame(
  v: VariantDef,
  rng: RNG,
  setupIndex = 0,
  mode: SetupMode = 'deck',
): GameState {
  const setup = drawSetup(v.players, setupIndex, mode);
  const dead = new Set(setup.outOfPlay);

  const systems = v.systems.map((s) => newSystemState(v.players, dead.has(s.cluster)));
  const playerStates = Array.from({ length: v.players }, () => newPlayerState());

  /** 5 tokens of each resource type in the general supply (p3). */
  const supply = {} as Record<ResourceType, number>;
  for (const r of RESOURCE_TYPES) supply[r] = 5;
  /** No Cartel card is in play at setup, so nothing sits on one. */
  const cartelZero = {} as Record<ResourceType, number>;
  for (const r of RESOURCE_TYPES) cartelZero[r] = 0;

  // p4 step B: the initiative marker goes to a random player, which is also
  // what decides who opens on which of the card's positions.
  const initiative = Math.floor(rng() * v.players);

  // p5 step N: 3 ships + city in A, 3 ships + starport in B, 2 ships in each C.
  setup.starts.forEach((start, position) => {
    const p = (initiative + position) % v.players;
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
    initiative,
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
