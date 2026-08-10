//! The map of the Reach (rulebook p6), mirroring `src/engine/map.ts`.
//!
//! 6 clusters, each 4 systems: 1 gate + 3 planets. Systems are indexed
//! `cluster * 4 + slot`, slot 0 being the gate. Adjacency:
//!
//! - each gate touches its cluster's 3 planets and its 2 neighbouring gates
//!   (the gates form a ring, cluster 1..6 clockwise);
//! - each planet touches its gate and one or both neighbouring planets — and
//!   a planet's neighbours run round the whole 18-planet ring, so two of them
//!   sit in the *next* cluster. See [`JOINED_BOUNDARIES`].
//!
//! Planet types, building-slot counts and the border reading are transcribed
//! from printed components — see the TS source and docs/DATA-GAPS.md.

use crate::inline_vec::InlineVec;
use crate::types::{ResourceType, SystemId};

pub const CLUSTER_COUNT: usize = 6;
pub const SYSTEMS_PER_CLUSTER: usize = 4;
pub const SYSTEM_COUNT: usize = CLUSTER_COUNT * SYSTEMS_PER_CLUSTER;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum SystemKind {
    Gate,
    Planet,
}

/// A system definition. `adjacent` holds base adjacency until
/// [`resolve_adjacency`] rewrites it for a game's out-of-play clusters.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SystemDef {
    pub id: SystemId,
    pub cluster: u8,
    /// 0 for the gate, 1..3 for the cluster's planets.
    pub slot: u8,
    pub kind: SystemKind,
    /// Planets only: the resource type taxed from cities here.
    pub planet_type: Option<ResourceType>,
    /// Planets only: how many buildings fit here (1 or 2).
    pub building_slots: u8,
    /// A gate touches at most 3 planets + 2 gates; capacity 6 leaves slack.
    pub adjacent: InlineVec<SystemId, 6>,
}

/// The printed label: "3" for a gate, "3.1" for a planet (1-based).
impl core::fmt::Display for SystemDef {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.kind {
            SystemKind::Gate => write!(f, "{}", self.cluster + 1),
            SystemKind::Planet => write!(f, "{}.{}", self.cluster + 1, self.slot),
        }
    }
}

#[derive(Clone, Copy)]
struct PlanetSpec {
    planet_type: ResourceType,
    slots: u8,
}

const fn planet(planet_type: ResourceType, slots: u8) -> PlanetSpec {
    PlanetSpec { planet_type, slots }
}

/// The 3 planets of each cluster, in slot order 1..3, transcribed from the
/// printed board (18 planets: Material x4, Fuel x4, Weapon x4, Relic x3,
/// Psionic x3).
const CLUSTERS: [[PlanetSpec; 3]; CLUSTER_COUNT] = {
    use ResourceType::{Fuel, Material, Psionic, Relic, Weapon};
    [
        [planet(Weapon, 2), planet(Fuel, 1), planet(Material, 2)],
        [planet(Psionic, 1), planet(Weapon, 1), planet(Relic, 2)],
        [planet(Material, 1), planet(Fuel, 1), planet(Weapon, 2)],
        [planet(Relic, 2), planet(Fuel, 2), planet(Material, 1)],
        [planet(Weapon, 1), planet(Relic, 1), planet(Psionic, 2)],
        [planet(Material, 1), planet(Fuel, 2), planet(Psionic, 1)],
    ]
};

/// The two cluster boundaries where the 18-planet ring closes without a thick
/// border: printed clusters 2.3–3.1 and 5.3–6.1 (0-based clusters 1 and 4).
pub const JOINED_BOUNDARIES: [u8; 2] = [1, 4];

#[inline]
pub const fn gate_id(cluster: u8) -> SystemId {
    SystemId(cluster * SYSTEMS_PER_CLUSTER as u8)
}

/// `planet` is 0..3 (slot minus 1), as in the TS source.
#[inline]
pub const fn planet_id(cluster: u8, planet: u8) -> SystemId {
    SystemId(cluster * SYSTEMS_PER_CLUSTER as u8 + 1 + planet)
}

#[inline]
pub const fn cluster_of(system: SystemId) -> u8 {
    system.0 / SYSTEMS_PER_CLUSTER as u8
}

#[inline]
pub const fn is_gate(system: SystemId) -> bool {
    system.0.is_multiple_of(SYSTEMS_PER_CLUSTER as u8)
}

/// Build the 24 system definitions with base (all-clusters-in-play)
/// adjacency.
pub fn build_systems() -> [SystemDef; SYSTEM_COUNT] {
    let joined = |c: u8| JOINED_BOUNDARIES.contains(&c);
    core::array::from_fn(|id| {
        let c = (id / SYSTEMS_PER_CLUSTER) as u8;
        let slot = (id % SYSTEMS_PER_CLUSTER) as u8;
        let n = CLUSTER_COUNT as u8;
        let prev = (c + n - 1) % n;
        let next = (c + 1) % n;
        let mut adjacent = InlineVec::new();
        if slot == 0 {
            for s in [
                gate_id(prev),
                gate_id(next),
                planet_id(c, 0),
                planet_id(c, 1),
                planet_id(c, 2),
            ] {
                adjacent.push(s);
            }
            SystemDef {
                id: SystemId(id as u8),
                cluster: c,
                slot,
                kind: SystemKind::Gate,
                planet_type: None,
                building_slots: 0,
                adjacent,
            }
        } else {
            let p = slot - 1;
            adjacent.push(gate_id(c));
            if p > 0 {
                adjacent.push(planet_id(c, p - 1));
            }
            if p < 2 {
                adjacent.push(planet_id(c, p + 1));
            }
            // The ring closes across two of the six cluster boundaries.
            if p == 2 && joined(c) {
                adjacent.push(planet_id(next, 0));
            }
            if p == 0 && joined(prev) {
                adjacent.push(planet_id(prev, 2));
            }
            let spec = CLUSTERS[c as usize][p as usize];
            SystemDef {
                id: SystemId(id as u8),
                cluster: c,
                slot,
                kind: SystemKind::Planet,
                planet_type: Some(spec.planet_type),
                building_slots: spec.slots,
                adjacent,
            }
        }
    })
}

/// Adjacency for a game, given which clusters are out of play.
///
/// Out-of-play systems are unreachable, and each out-of-play gate becomes a
/// **path marker** joining its two neighbouring gates so ships cross the gap
/// in a single move (p6). Two adjacent out-of-play clusters chain their
/// paths.
pub fn resolve_adjacency(
    systems: &[SystemDef; SYSTEM_COUNT],
    out_of_play: &[u8],
) -> [SystemDef; SYSTEM_COUNT] {
    let dead = |c: u8| out_of_play.contains(&c);
    let n = CLUSTER_COUNT as u8;

    core::array::from_fn(|i| {
        let mut s = systems[i];
        if dead(s.cluster) {
            s.adjacent = InlineVec::new();
            return s;
        }
        if s.kind != SystemKind::Gate {
            s.adjacent.retain(|&a| !dead(cluster_of(a)));
            return s;
        }
        // Gates: walk around the ring past any run of out-of-play clusters.
        let mut reachable = s.adjacent;
        reachable.retain(|&a| !dead(cluster_of(a)));
        for dir in [-1i8, 1i8] {
            let step = |c: u8| (c as i8 + dir).rem_euclid(n as i8) as u8;
            let mut c = step(s.cluster);
            let mut steps = 0;
            while dead(c) && steps < CLUSTER_COUNT {
                c = step(c);
                steps += 1;
            }
            if c != s.cluster && !dead(c) && !reachable.contains(&gate_id(c)) {
                reachable.push(gate_id(c));
            }
        }
        s.adjacent = reachable;
        s
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn adjacent(systems: &[SystemDef; SYSTEM_COUNT], id: u8) -> Vec<u8> {
        let mut v: Vec<u8> = systems[id as usize].adjacent.iter().map(|s| s.0).collect();
        v.sort_unstable();
        v
    }

    #[test]
    fn base_adjacency_is_symmetric() {
        let systems = build_systems();
        for s in &systems {
            for a in &s.adjacent {
                assert!(
                    systems[a.as_index()].adjacent.contains(&s.id),
                    "asymmetric edge {:?} -> {:?}",
                    s.id,
                    a
                );
            }
        }
    }

    #[test]
    fn joined_boundaries_close_the_planet_ring() {
        let systems = build_systems();
        // Cluster 1's last planet (id 7) touches cluster 2's first (id 9).
        assert!(systems[7].adjacent.contains(&SystemId(9)));
        assert!(systems[9].adjacent.contains(&SystemId(7)));
        // Cluster 4's last planet (id 19) touches cluster 5's first (id 21).
        assert!(systems[19].adjacent.contains(&SystemId(21)));
        // A thick boundary does not close: cluster 0's last planet (id 3)
        // does not touch cluster 1's first (id 5).
        assert!(!systems[3].adjacent.contains(&SystemId(5)));
    }

    #[test]
    fn path_markers_bridge_a_single_gap() {
        let systems = build_systems();
        let resolved = resolve_adjacency(&systems, &[2]);
        // Gate 1 (id 4) reaches gate 3 (id 12) across dead cluster 2.
        assert!(resolved[4].adjacent.contains(&SystemId(12)));
        assert!(resolved[12].adjacent.contains(&SystemId(4)));
        // Dead systems are unreachable and have no edges.
        for id in 8..12 {
            assert!(resolved[id].adjacent.is_empty());
            for s in &resolved {
                assert!(!s.adjacent.contains(&SystemId(id as u8)));
            }
        }
    }

    #[test]
    fn path_markers_chain_across_adjacent_gaps() {
        let systems = build_systems();
        // Clusters 1 and 2 both out: gate 0 must reach gate 3 (id 12) in one
        // hop, chained through both path markers.
        let resolved = resolve_adjacency(&systems, &[1, 2]);
        assert_eq!(adjacent(&resolved, 0), vec![1, 2, 3, 12, 20]);
        assert_eq!(adjacent(&resolved, 12), vec![0, 13, 14, 15, 16]);
    }

    #[test]
    fn resolved_adjacency_stays_symmetric() {
        let systems = build_systems();
        for out in [&[1u8, 4][..], &[0, 3], &[2], &[1, 2], &[0, 5], &[4, 5]] {
            let resolved = resolve_adjacency(&systems, out);
            for s in &resolved {
                for a in &s.adjacent {
                    assert!(resolved[a.as_index()].adjacent.contains(&s.id));
                }
            }
        }
    }
}
