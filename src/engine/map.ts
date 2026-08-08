/**
 * The map of the Reach (rulebook p6).
 *
 * 6 clusters, each 4 systems: 1 gate + 3 planets. Systems are indexed
 * `cluster * 4 + slot`, slot 0 being the gate. Adjacency:
 *
 *   - each gate touches its cluster's 3 planets and its 2 neighbouring gates
 *     (the gates form a ring, cluster 1..6 clockwise);
 *   - each planet touches its gate and one or both neighbouring planets.
 *
 * Planet types and building-slot counts below are transcribed from the board
 * illustration on p4 of the high-resolution rulebook — see docs/DATA-GAPS.md §2
 * for the extraction recipe and the badge legend.
 *
 * DATA-GAP (residual): which intra-cluster planet pair is separated by a thick
 * border is still modelled uniformly as a path (planet 1-2 and 2-3 adjacent,
 * 1-3 not), which satisfies "adjacent to one or both neighbouring planets" (p6)
 * but is not read off the board per cluster.
 */
import type { ResourceType, SystemDef } from './types';

export const CLUSTER_COUNT = 6;
export const SYSTEMS_PER_CLUSTER = 4;

interface PlanetDef {
  type: ResourceType;
  slots: number;
}

/**
 * The 3 planets of each cluster, in slot order 1..3, transcribed from the
 * printed board. Within a wedge the planets are listed in increasing angle
 * (clockwise), matching the gate numbering: gate 1 at the top, 2..6 clockwise.
 *
 * 18 planets: Material ×4, Fuel ×4, Weapon ×4, Relic ×3, Psionic ×3. Relic and
 * Psionic are the scarce types, which is why Keeper and Empath are the cheap
 * ambitions to contest. `components.test.ts` asserts this distribution.
 */
const CLUSTERS: PlanetDef[][] = [
  [
    { type: 'weapon', slots: 2 },
    { type: 'fuel', slots: 1 },
    { type: 'material', slots: 2 },
  ],
  [
    { type: 'psionic', slots: 1 },
    { type: 'weapon', slots: 1 },
    { type: 'relic', slots: 2 },
  ],
  [
    { type: 'material', slots: 1 },
    { type: 'fuel', slots: 1 },
    { type: 'weapon', slots: 2 },
  ],
  [
    { type: 'relic', slots: 2 },
    { type: 'fuel', slots: 2 },
    { type: 'material', slots: 1 },
  ],
  [
    { type: 'weapon', slots: 1 },
    { type: 'relic', slots: 1 },
    { type: 'psionic', slots: 2 },
  ],
  [
    { type: 'material', slots: 1 },
    { type: 'fuel', slots: 2 },
    { type: 'psionic', slots: 1 },
  ],
];

export function gateId(cluster: number): number {
  return cluster * SYSTEMS_PER_CLUSTER;
}

export function planetId(cluster: number, planet: number): number {
  return cluster * SYSTEMS_PER_CLUSTER + 1 + planet;
}

export function clusterOf(system: number): number {
  return Math.floor(system / SYSTEMS_PER_CLUSTER);
}

export function isGate(system: number): boolean {
  return system % SYSTEMS_PER_CLUSTER === 0;
}

/** Build the 24 system definitions with base (all-clusters-in-play) adjacency. */
export function buildSystems(): SystemDef[] {
  const systems: SystemDef[] = [];
  for (let c = 0; c < CLUSTER_COUNT; c++) {
    const prev = (c + CLUSTER_COUNT - 1) % CLUSTER_COUNT;
    const next = (c + 1) % CLUSTER_COUNT;
    systems.push({
      id: gateId(c),
      cluster: c,
      slot: 0,
      kind: 'gate',
      planetType: null,
      buildingSlots: 0,
      adjacent: [gateId(prev), gateId(next), planetId(c, 0), planetId(c, 1), planetId(c, 2)],
      label: `${c + 1}`,
    });
    for (let p = 0; p < 3; p++) {
      const neighbours = [gateId(c)];
      if (p > 0) neighbours.push(planetId(c, p - 1));
      if (p < 2) neighbours.push(planetId(c, p + 1));
      systems.push({
        id: planetId(c, p),
        cluster: c,
        slot: p + 1,
        kind: 'planet',
        planetType: CLUSTERS[c][p].type,
        buildingSlots: CLUSTERS[c][p].slots,
        adjacent: neighbours,
        label: `${c + 1}.${p + 1}`,
      });
    }
  }
  return systems;
}

/**
 * Adjacency for a game, given which clusters are out of play.
 *
 * Out-of-play systems are unreachable, and each out-of-play gate becomes a
 * **path marker** joining its two neighbouring gates so ships cross the gap in
 * a single move (p6). Two adjacent out-of-play clusters chain their paths.
 */
export function resolveAdjacency(systems: SystemDef[], outOfPlay: number[]): SystemDef[] {
  const dead = new Set(outOfPlay);
  const resolved = systems.map((s) => ({ ...s, adjacent: s.adjacent.slice() }));

  for (const s of resolved) {
    if (dead.has(s.cluster)) {
      s.adjacent = [];
      continue;
    }
    if (s.kind !== 'gate') {
      s.adjacent = s.adjacent.filter((n) => !dead.has(clusterOf(n)));
      continue;
    }
    // Gates: walk around the ring past any run of out-of-play clusters.
    const reachable = s.adjacent.filter((n) => !dead.has(clusterOf(n)));
    for (const dir of [-1, 1]) {
      let c = (s.cluster + dir + CLUSTER_COUNT) % CLUSTER_COUNT;
      let steps = 0;
      while (dead.has(c) && steps++ < CLUSTER_COUNT) {
        c = (c + dir + CLUSTER_COUNT) % CLUSTER_COUNT;
      }
      if (c !== s.cluster && !dead.has(c) && !reachable.includes(gateId(c))) {
        reachable.push(gateId(c));
      }
    }
    s.adjacent = reachable;
  }
  return resolved;
}
