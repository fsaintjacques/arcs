//! Battle dice (rulebook p14, faces from the official Arcs aid booklet p3),
//! mirroring `src/engine/dice.ts`. The aid booklet prints all six faces of
//! each die, so these tables are exact.

use crate::inline_vec::InlineVec;
use crate::rng::Rng;
use crate::types::{ByDieType, DieType};

/// One face of a battle die.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct DieFace {
    /// Hits the attacker must take on their own attacking ships.
    pub self_hits: u8,
    /// Triggers the defender's interception (resolved once per battle).
    pub intercept: u8,
    /// Hits on defending ships, spilling onto buildings once ships are gone.
    pub hits: u8,
    /// Hits that only ever land on defending buildings.
    pub building_hits: u8,
    /// Keys spent to steal resources and Guild cards.
    pub keys: u8,
}

const fn face(hits: u8, self_hits: u8, building_hits: u8, keys: u8, intercept: u8) -> DieFace {
    DieFace {
        self_hits,
        intercept,
        hits,
        building_hits,
        keys,
    }
}

pub const FACES_PER_DIE: usize = 6;

/// Three faces with a single hit, three blank. Never hurts the attacker.
pub const SKIRMISH_FACES: [DieFace; FACES_PER_DIE] = [
    face(1, 0, 0, 0, 0),
    face(1, 0, 0, 0, 0),
    face(1, 0, 0, 0, 0),
    face(0, 0, 0, 0, 0),
    face(0, 0, 0, 0, 0),
    face(0, 0, 0, 0, 0),
];

/// 2 hits · 2 hits + self-hit · hit + intercept · hit + self-hit ·
/// hit + self-hit · blank.
pub const ASSAULT_FACES: [DieFace; FACES_PER_DIE] = [
    face(2, 0, 0, 0, 0),
    face(2, 1, 0, 0, 0),
    face(1, 0, 0, 0, 1),
    face(1, 1, 0, 0, 0),
    face(1, 1, 0, 0, 0),
    face(0, 0, 0, 0, 0),
];

/// 2 keys + intercept · key + self-hit · building hit + key ·
/// building hit + self-hit · building hit + self-hit · intercept.
///
/// Raid dice never damage defending ships, which is why the Raid Dice Limit
/// (p14) gates them on the defender having buildings to lose.
pub const RAID_FACES: [DieFace; FACES_PER_DIE] = [
    face(0, 0, 0, 2, 1),
    face(0, 1, 0, 1, 0),
    face(0, 0, 1, 1, 0),
    face(0, 1, 1, 0, 0),
    face(0, 1, 1, 0, 0),
    face(0, 0, 0, 0, 1),
];

/// Faces by die type, indexed by [`DieType::as_index`] (assault, skirmish,
/// raid).
pub const DIE_FACES: ByDieType<[DieFace; FACES_PER_DIE]> =
    [ASSAULT_FACES, SKIRMISH_FACES, RAID_FACES];

/// Dice of each type in the box (p3) — the per-type collection cap (p14).
pub const DICE_PER_TYPE: usize = 6;

pub fn roll_die(die: DieType, rng: &mut impl Rng) -> &'static DieFace {
    &DIE_FACES[die.as_index()][rng.gen_range(FACES_PER_DIE)]
}

/// Rolled totals, consumed as they are assigned during battle resolution.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct RollTotals {
    pub self_hits: u8,
    pub intercept: u8,
    pub hits: u8,
    pub building_hits: u8,
    pub keys: u8,
    /// Skirmish dice that came up blank — what Skirmishers may reroll.
    pub skirmish_blanks: u8,
}

/// A battle roll: the totals plus the face each die landed on (indices into
/// `DIE_FACES[type]`), so the UI can draw the roll as the dice sit.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct RollResult {
    pub totals: RollTotals,
    pub faces: ByDieType<InlineVec<u8, DICE_PER_TYPE>>,
}

/// Roll `dice[type]` dice of each type (each at most [`DICE_PER_TYPE`]).
pub fn roll_battle(dice: ByDieType<u8>, rng: &mut impl Rng) -> RollResult {
    let mut result = RollResult::default();
    for die in DieType::ALL {
        let faces = &DIE_FACES[die.as_index()];
        for _ in 0..dice[die.as_index()] {
            let idx = rng.gen_range(FACES_PER_DIE);
            let f = &faces[idx];
            result.faces[die.as_index()].push(idx as u8);
            result.totals.self_hits += f.self_hits;
            result.totals.intercept += f.intercept;
            result.totals.hits += f.hits;
            result.totals.building_hits += f.building_hits;
            result.totals.keys += f.keys;
            if die == DieType::Skirmish && f.hits == 0 {
                result.totals.skirmish_blanks += 1;
            }
        }
    }
    result
}

/// The result of a Skirmishers reroll of blank skirmish dice.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct RerollResult {
    pub hits: u8,
    pub blanks: u8,
    pub faces: InlineVec<u8, DICE_PER_TYPE>,
}

/// Reroll `count` blank skirmish dice. Only blanks are ever rerolled — see
/// the ruling in game.ts — so a reroll can only ever help.
pub fn reroll_skirmish(count: u8, rng: &mut impl Rng) -> RerollResult {
    let mut result = RerollResult::default();
    for _ in 0..count {
        let idx = rng.gen_range(FACES_PER_DIE);
        result.faces.push(idx as u8);
        let hits = SKIRMISH_FACES[idx].hits;
        if hits > 0 {
            result.hits += hits;
        } else {
            result.blanks += 1;
        }
    }
    result
}

/// Expected values per die, for heuristic bots. (`skirmish_blanks` is not
/// accumulated, matching the TS `expectedFace`.)
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct ExpectedFace {
    pub self_hits: f64,
    pub intercept: f64,
    pub hits: f64,
    pub building_hits: f64,
    pub keys: f64,
}

pub fn expected_face(die: DieType) -> ExpectedFace {
    let faces = &DIE_FACES[die.as_index()];
    let n = faces.len() as f64;
    let mut acc = ExpectedFace::default();
    for f in faces {
        acc.self_hits += f.self_hits as f64 / n;
        acc.intercept += f.intercept as f64 / n;
        acc.hits += f.hits as f64 / n;
        acc.building_hits += f.building_hits as f64 / n;
        acc.keys += f.keys as f64 / n;
    }
    acc
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rng::SplitMix64;

    #[test]
    fn expected_values_match_hand_computation() {
        let skirmish = expected_face(DieType::Skirmish);
        assert!((skirmish.hits - 0.5).abs() < 1e-12);
        assert!(skirmish.self_hits.abs() < 1e-12);
        let assault = expected_face(DieType::Assault);
        assert!((assault.hits - 7.0 / 6.0).abs() < 1e-12);
        assert!((assault.self_hits - 3.0 / 6.0).abs() < 1e-12);
        let raid = expected_face(DieType::Raid);
        assert!((raid.keys - 4.0 / 6.0).abs() < 1e-12);
        assert!((raid.building_hits - 3.0 / 6.0).abs() < 1e-12);
        assert!((raid.intercept - 2.0 / 6.0).abs() < 1e-12);
    }

    #[test]
    fn roll_battle_totals_match_faces() {
        let mut rng = SplitMix64::new(11);
        for _ in 0..200 {
            let dice = [
                rng.gen_range(7) as u8,
                rng.gen_range(7) as u8,
                rng.gen_range(7) as u8,
            ];
            let roll = roll_battle(dice, &mut rng);
            let mut expect = RollTotals::default();
            for die in DieType::ALL {
                assert_eq!(
                    roll.faces[die.as_index()].len(),
                    dice[die.as_index()] as usize
                );
                for &idx in roll.faces[die.as_index()].iter() {
                    let f = &DIE_FACES[die.as_index()][idx as usize];
                    expect.self_hits += f.self_hits;
                    expect.intercept += f.intercept;
                    expect.hits += f.hits;
                    expect.building_hits += f.building_hits;
                    expect.keys += f.keys;
                    if die == DieType::Skirmish && f.hits == 0 {
                        expect.skirmish_blanks += 1;
                    }
                }
            }
            assert_eq!(roll.totals, expect);
        }
    }

    #[test]
    fn reroll_only_counts_hits_and_blanks() {
        let mut rng = SplitMix64::new(3);
        for _ in 0..100 {
            let r = reroll_skirmish(6, &mut rng);
            assert_eq!((r.hits + r.blanks) as usize, 6);
            assert_eq!(r.faces.len(), 6);
        }
    }
}
