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
 * DATA-GAP: which planet type sits where, how many building slots each planet
 * has, and which intra-cluster planet pairs share a thin border are not in the
 * rulebook. The layout below is *a* legal Reach, not *the* Reach — see
 * docs/DATA-GAPS.md §3. Replace `CLUSTERS` to use the printed map.
 */
import type { ResourceType, SystemDef } from './types';

export const CLUSTER_COUNT = 6;
export const SYSTEMS_PER_CLUSTER = 4;

interface PlanetDef {
  type: ResourceType;
  slots: number;
}

/**
 * The 3 planets of each cluster, in slot order 1..3. Planet 1-2 and 2-3 are
 * adjacent; 1-3 are separated by a thick border (p6).
 *
 * 18 planets over 5 types: material/fuel/relic/psionic ×4 is 16, weapon ×2 —
 * weapon planets are deliberately scarce because Weapon Guild cards score no
 * ambition (p17), so weapon worlds are a means, not an end.
 */
// DATA-GAP: invented layout, see docs/DATA-GAPS.md §3.
const CLUSTERS: PlanetDef[][] = [
  [
    { type: 'material', slots: 2 },
    { type: 'fuel', slots: 1 },
    { type: 'psionic', slots: 1 },
  ],
  [
    { type: 'fuel', slots: 2 },
    { type: 'relic', slots: 1 },
    { type: 'material', slots: 1 },
  ],
  [
    { type: 'material', slots: 2 },
    { type: 'weapon', slots: 1 },
    { type: 'relic', slots: 1 },
  ],
  [
    { type: 'fuel', slots: 2 },
    { type: 'psionic', slots: 1 },
    { type: 'material', slots: 1 },
  ],
  [
    { type: 'relic', slots: 2 },
    { type: 'fuel', slots: 1 },
    { type: 'psionic', slots: 1 },
  ],
  [
    { type: 'psionic', slots: 2 },
    { type: 'weapon', slots: 1 },
    { type: 'relic', slots: 1 },
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
