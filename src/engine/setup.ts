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
 * The setup-card deck: 4 cards per player count, as the box has (p4 step I).
 *
 * DATA-GAP: the printed 12 are not in the rulebook. These are reconstructed —
 * a large pool of legal draws scored for balance (equal building capacity per
 * player, two distinct resources each, no two players crowded into neighbouring
 * clusters) with the best four kept per count, chosen to differ in which
 * clusters they take out of play. `tools/pick-setups.ts` regenerates them.
 *
 * The 2-player cards come out perfectly balanced on that score; 3 and 4 players
 * cannot, because 4 or 5 live clusters will not seat that many players an equal
 * distance apart. That is a property of the map, not of the search.
 *
 * Positions are numbered as on a printed card — `starts[0]` is 1A/1B/1C — and
 * `drawSetup` assigns players to them at random, so a seat does not imply a
 * position.
 */
export const SETUP_DECK: Record<number, SetupCard[]> = {
  2: [
    { name: 'Frontiers', outOfPlay: [0, 3], starts: [{ a: 21, b: 17, c: [20, 16] }, { a: 10, b: 9, c: [8, 4] }] },
    { name: 'Mirrors', outOfPlay: [4, 5], starts: [{ a: 13, b: 9, c: [12, 8] }, { a: 1, b: 2, c: [0, 4] }] },
    { name: 'Crossroads', outOfPlay: [0, 2], starts: [{ a: 7, b: 6, c: [4, 20] }, { a: 17, b: 14, c: [16, 12] }] },
    { name: 'Verge', outOfPlay: [1, 4], starts: [{ a: 22, b: 23, c: [20, 0] }, { a: 10, b: 11, c: [8, 12] }] },
  ],
  3: [
    { name: 'Triangulum', outOfPlay: [1, 5], starts: [{ a: 10, b: 11, c: [8] }, { a: 18, b: 14, c: [16] }, { a: 1, b: 2, c: [0] }] },
    { name: 'Wheelhouse', outOfPlay: [3, 5], starts: [{ a: 3, b: 5, c: [0] }, { a: 19, b: 18, c: [16] }, { a: 9, b: 11, c: [8] }] },
    { name: 'Divide', outOfPlay: [0, 4], starts: [{ a: 13, b: 15, c: [12] }, { a: 7, b: 6, c: [4] }, { a: 22, b: 21, c: [20] }] },
    { name: 'Reaches', outOfPlay: [2, 4], starts: [{ a: 23, b: 22, c: [20] }, { a: 5, b: 7, c: [4] }, { a: 15, b: 14, c: [12] }] },
  ],
  4: [
    { name: 'Quadrants', outOfPlay: [2], starts: [{ a: 19, b: 21, c: [16] }, { a: 22, b: 17, c: [20] }, { a: 13, b: 15, c: [12] }, { a: 5, b: 7, c: [4] }] },
    { name: 'Bastions', outOfPlay: [5], starts: [{ a: 3, b: 5, c: [0] }, { a: 15, b: 13, c: [12] }, { a: 9, b: 11, c: [8] }, { a: 18, b: 19, c: [16] }] },
    { name: 'Sprawl', outOfPlay: [4], starts: [{ a: 1, b: 23, c: [0] }, { a: 22, b: 21, c: [20] }, { a: 14, b: 15, c: [12] }, { a: 11, b: 9, c: [8] }] },
    { name: 'Crown', outOfPlay: [1], starts: [{ a: 19, b: 21, c: [16] }, { a: 14, b: 15, c: [12] }, { a: 3, b: 23, c: [0] }, { a: 11, b: 9, c: [8] }] },
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
 * Choose the opening. `seed` picks the card and the seating on it, so the same
 * seed always reproduces the same board — which is what lets `makeVariant` and
 * `newGame` derive it independently and still agree.
 *
 * **Which player gets which position on the card is randomised.** Both are
 * things the rulebook leaves outside the card: the card labels its positions
 * 1A/1B/1C, 2A/2B/2C and so on, and nothing says seat 0 takes position 1. Turn
 * order is randomised separately, by the initiative marker.
 */
export function drawSetup(players: number, seed: number, mode: SetupMode = 'deck'): SetupCard {
  if (mode === 'draw') return generateSetup(players, seed);

  const rng = mulberry32((Math.abs(Math.trunc(seed)) * 2246822519 + 0xc0de) >>> 0);
  const deck = SETUP_DECK[players];
  const card = pick(deck, rng);

  // Seat p takes the card position `order[p]`.
  const order = shuffle(
    Array.from({ length: players }, (_, i) => i),
    rng,
  );
  return {
    name: card.name,
    outOfPlay: card.outOfPlay,
    starts: order.map((i) => card.starts[i]),
  };
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
