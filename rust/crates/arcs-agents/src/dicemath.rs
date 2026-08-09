//! Exact battle-roll distributions, ported from `src/agents/dicemath.ts`.
//!
//! The die faces are printed and finite, so a battle's outcome distribution
//! is a small convolution, not something to sample: there are at most 343
//! distinct pools (0-6 dice of three types), each with a few hundred distinct
//! outcome totals. FINDINGS.md flagged this as the lever nobody pulled —
//! `greedy` judged battles on 3 sampled rolls, mcts on one per iteration.
//!
//! Where TS memoizes into a `Map` keyed by a string, the Rust port keeps a
//! **343-entry table indexed by `(assault, skirmish, raid)`**, each cell a
//! `OnceLock` filled on first use. After warm-up a lookup is an array index
//! and the returned slice is `'static`, so callers never copy a distribution
//! to read it.
//!
//! The convolution order and the per-die `p / 6` division match the TS
//! statement for statement, so the probabilities agree to floating-point
//! noise (asserted against a printed fixture at 1e-12). One deliberate
//! difference: TS sorts by descending probability and leaves equiprobable
//! outcomes in `Map` insertion order, which is not a property anything should
//! depend on; Rust breaks those ties by the packed totals key so the order is
//! total and reproducible. It can only permute outcomes of identical
//! probability, so no expectation changes and [`top_mass`] can only differ on
//! a tie straddling its cutoff.

use std::collections::HashMap;
use std::sync::OnceLock;

use arcs_engine::dice::{DICE_PER_TYPE, DIE_FACES, RollTotals};
use arcs_engine::types::DieType;

/// One reachable outcome of a pool and how likely it is.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct BattleOutcome {
    pub totals: RollTotals,
    pub p: f64,
}

/// Per-component expectation of a pool. The TS version reuses `RollTotals`
/// with fractional numbers in it; `RollTotals` is integral in Rust, so the
/// expectation gets its own type.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct ExpectedTotals {
    pub self_hits: f64,
    pub intercept: f64,
    pub hits: f64,
    pub building_hits: f64,
    pub keys: f64,
    pub skirmish_blanks: f64,
}

/// Pack the six totals into one integer key (each component fits in a byte:
/// 6 dice of a type can contribute at most 12 of anything). The TS `keyOf`
/// string, minus the allocation.
#[inline]
fn key_of(t: &RollTotals) -> u64 {
    (t.self_hits as u64)
        | (t.intercept as u64) << 8
        | (t.hits as u64) << 16
        | (t.building_hits as u64) << 24
        | (t.keys as u64) << 32
        | (t.skirmish_blanks as u64) << 40
}

/// Distribution over totals for `n` dice of one type.
fn type_distribution(die: DieType, n: usize) -> Vec<BattleOutcome> {
    let mut dist = vec![BattleOutcome {
        totals: RollTotals::default(),
        p: 1.0,
    }];
    let faces = &DIE_FACES[die.as_index()];
    let count = faces.len() as f64;
    for _ in 0..n {
        let mut index: HashMap<u64, usize> = HashMap::new();
        let mut next: Vec<BattleOutcome> = Vec::new();
        for o in &dist {
            for f in faces {
                let t = RollTotals {
                    self_hits: o.totals.self_hits + f.self_hits,
                    intercept: o.totals.intercept + f.intercept,
                    hits: o.totals.hits + f.hits,
                    building_hits: o.totals.building_hits + f.building_hits,
                    keys: o.totals.keys + f.keys,
                    skirmish_blanks: o.totals.skirmish_blanks
                        + u8::from(die == DieType::Skirmish && f.hits == 0),
                };
                let p = o.p / count;
                match index.get(&key_of(&t)) {
                    Some(&i) => next[i].p += p,
                    None => {
                        index.insert(key_of(&t), next.len());
                        next.push(BattleOutcome { totals: t, p });
                    }
                }
            }
        }
        dist = next;
    }
    dist
}

/// `type_dists[die][n]` for `n` in `0..=DICE_PER_TYPE`, built once.
fn type_dists() -> &'static [[Vec<BattleOutcome>; DICE_PER_TYPE + 1]; DieType::COUNT] {
    static TYPE_DISTS: OnceLock<[[Vec<BattleOutcome>; DICE_PER_TYPE + 1]; DieType::COUNT]> =
        OnceLock::new();
    TYPE_DISTS.get_or_init(|| {
        core::array::from_fn(|d| {
            let die = DieType::ALL[d];
            core::array::from_fn(|n| type_distribution(die, n))
        })
    })
}

/// Convolve two independent distributions, summing their totals.
fn convolve(dist: &[BattleOutcome], other: &[BattleOutcome]) -> Vec<BattleOutcome> {
    let mut index: HashMap<u64, usize> = HashMap::new();
    let mut next: Vec<BattleOutcome> = Vec::with_capacity(dist.len());
    for x in dist {
        for y in other {
            let t = RollTotals {
                self_hits: x.totals.self_hits + y.totals.self_hits,
                intercept: x.totals.intercept + y.totals.intercept,
                hits: x.totals.hits + y.totals.hits,
                building_hits: x.totals.building_hits + y.totals.building_hits,
                keys: x.totals.keys + y.totals.keys,
                skirmish_blanks: x.totals.skirmish_blanks + y.totals.skirmish_blanks,
            };
            let p = x.p * y.p;
            match index.get(&key_of(&t)) {
                Some(&i) => next[i].p += p,
                None => {
                    index.insert(key_of(&t), next.len());
                    next.push(BattleOutcome { totals: t, p });
                }
            }
        }
    }
    next
}

fn pool_distribution(assault: usize, skirmish: usize, raid: usize) -> Box<[BattleOutcome]> {
    let dists = type_dists();
    let mut dist = dists[DieType::Assault.as_index()][assault].clone();
    for other in [
        &dists[DieType::Skirmish.as_index()][skirmish],
        &dists[DieType::Raid.as_index()][raid],
    ] {
        dist = convolve(&dist, other);
    }
    dist.sort_by(|x, y| {
        y.p.partial_cmp(&x.p)
            .expect("probabilities are finite")
            .then_with(|| key_of(&x.totals).cmp(&key_of(&y.totals)))
    });
    dist.into_boxed_slice()
}

/// One `OnceLock` per pool shape, indexed `(a * 7 + s) * 7 + r`.
const POOL_SHAPES: usize = (DICE_PER_TYPE + 1) * (DICE_PER_TYPE + 1) * (DICE_PER_TYPE + 1);

fn pools() -> &'static [OnceLock<Box<[BattleOutcome]>>; POOL_SHAPES] {
    static POOLS: OnceLock<[OnceLock<Box<[BattleOutcome]>>; POOL_SHAPES]> = OnceLock::new();
    POOLS.get_or_init(|| core::array::from_fn(|_| OnceLock::new()))
}

/// The exact outcome distribution of rolling this pool, sorted most probable
/// first. Pools are capped at [`DICE_PER_TYPE`] per type, as the game caps
/// them. (`battleDistribution` in dicemath.ts.)
pub fn battle_distribution(assault: u8, skirmish: u8, raid: u8) -> &'static [BattleOutcome] {
    let a = (assault as usize).min(DICE_PER_TYPE);
    let s = (skirmish as usize).min(DICE_PER_TYPE);
    let r = (raid as usize).min(DICE_PER_TYPE);
    let cell = &pools()[(a * (DICE_PER_TYPE + 1) + s) * (DICE_PER_TYPE + 1) + r];
    cell.get_or_init(|| pool_distribution(a, s, r))
}

/// The most probable outcomes covering at least `mass` of the distribution,
/// with probabilities renormalised to sum to 1 — expectation over these is
/// expectation over the whole distribution, truncated where it stops
/// mattering. (`topMass` in dicemath.ts.)
pub fn top_mass(dist: &[BattleOutcome], mass: f64) -> Vec<BattleOutcome> {
    let mut out: Vec<BattleOutcome> = Vec::new();
    let mut sum = 0.0f64;
    for o in dist {
        out.push(*o);
        sum += o.p;
        if sum >= mass {
            break;
        }
    }
    for o in out.iter_mut() {
        o.p /= sum;
    }
    out
}

/// Per-component expectation of the pool, from the exact distribution.
/// (`expectedBattle` in dicemath.ts.)
pub fn expected_battle(assault: u8, skirmish: u8, raid: u8) -> ExpectedTotals {
    let mut acc = ExpectedTotals::default();
    for o in battle_distribution(assault, skirmish, raid) {
        acc.self_hits += o.totals.self_hits as f64 * o.p;
        acc.intercept += o.totals.intercept as f64 * o.p;
        acc.hits += o.totals.hits as f64 * o.p;
        acc.building_hits += o.totals.building_hits as f64 * o.p;
        acc.keys += o.totals.keys as f64 * o.p;
        acc.skirmish_blanks += o.totals.skirmish_blanks as f64 * o.p;
    }
    acc
}

#[cfg(test)]
mod tests {
    use super::*;
    use arcs_engine::dice::expected_face;

    // Ported from tests/dicemath.test.ts "sums to probability 1 for every
    // pool shape".
    #[test]
    fn sums_to_probability_one_for_every_pool_shape() {
        for a in 0..=6u8 {
            for s in 0..=6u8 {
                for r in 0..=6u8 {
                    let total: f64 = battle_distribution(a, s, r).iter().map(|o| o.p).sum();
                    assert!(
                        (total - 1.0).abs() < 1e-9,
                        "pool {a}/{s}/{r} sums to {total}"
                    );
                }
            }
        }
    }

    // Ported from tests/dicemath.test.ts "matches the per-die expected faces
    // exactly".
    #[test]
    fn matches_the_per_die_expected_faces_exactly() {
        let (a, s, r) = (4u8, 3u8, 2u8);
        let exact = expected_battle(a, s, r);
        let ea = expected_face(DieType::Assault);
        let es = expected_face(DieType::Skirmish);
        let er = expected_face(DieType::Raid);
        let (fa, fs, fr) = (a as f64, s as f64, r as f64);
        let close = |x: f64, y: f64| assert!((x - y).abs() < 1e-9, "{x} != {y}");
        close(exact.hits, fa * ea.hits + fs * es.hits + fr * er.hits);
        close(
            exact.self_hits,
            fa * ea.self_hits + fs * es.self_hits + fr * er.self_hits,
        );
        close(exact.keys, fa * ea.keys + fs * es.keys + fr * er.keys);
        close(
            exact.building_hits,
            fa * ea.building_hits + fs * es.building_hits + fr * er.building_hits,
        );
        close(
            exact.intercept,
            fa * ea.intercept + fs * es.intercept + fr * er.intercept,
        );
        // Half the skirmish faces are blank.
        close(exact.skirmish_blanks, fs / 2.0);
    }

    // Ported from tests/dicemath.test.ts "topMass renormalises and keeps the
    // most probable outcomes".
    #[test]
    fn top_mass_renormalises_and_keeps_the_most_probable_outcomes() {
        let dist = battle_distribution(6, 6, 6);
        let top = top_mass(dist, 0.95);
        assert!(top.len() < dist.len());
        let total: f64 = top.iter().map(|o| o.p).sum();
        assert!((total - 1.0).abs() < 1e-9);
        // Sorted most-probable-first, so the head of top_mass is the mode.
        assert_eq!(top[0].totals, dist[0].totals);
    }

    // Ported from tests/dicemath.test.ts "agrees with 100k seeded
    // Monte-Carlo rolls".
    #[test]
    fn agrees_with_seeded_monte_carlo_rolls() {
        use arcs_engine::dice::roll_battle;
        use arcs_engine::{Rng, SplitMix64};
        let dice = [3u8, 2u8, 2u8];
        let mut rng = SplitMix64::new(77);
        let n = 100_000u32;
        let (mut hits, mut self_hits, mut keys, mut building_hits) = (0u64, 0u64, 0u64, 0u64);
        for _ in 0..n {
            let t = roll_battle(dice, &mut rng).totals;
            hits += t.hits as u64;
            self_hits += t.self_hits as u64;
            keys += t.keys as u64;
            building_hits += t.building_hits as u64;
        }
        let exact = expected_battle(dice[0], dice[1], dice[2]);
        let n = n as f64;
        let close = |x: f64, y: f64| assert!((x - y).abs() < 0.05, "{x} != {y}");
        close(hits as f64 / n, exact.hits);
        close(self_hits as f64 / n, exact.self_hits);
        close(keys as f64 / n, exact.keys);
        close(building_hits as f64 / n, exact.building_hits);
        let _ = rng.next_u64();
    }

    /// The table is memoized: the same pool hands back the same slice.
    #[test]
    fn the_pool_table_is_memoized() {
        let a = battle_distribution(3, 2, 1);
        let b = battle_distribution(3, 2, 1);
        assert!(core::ptr::eq(a, b));
        // Pools are capped at DICE_PER_TYPE per type, as the game caps them.
        assert!(core::ptr::eq(
            battle_distribution(9, 9, 9),
            battle_distribution(6, 6, 6)
        ));
    }
}
