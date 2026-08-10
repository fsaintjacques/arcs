//! Variant construction and game setup (rulebook p4-p5), mirroring
//! `src/engine/setup.ts`.
//!
//! [`SETUP_DECK`] is the printed 12, transcribed from the cards.
//! [`generate_setup`] invents legal openings instead, for measurement batches
//! that want more than four boards per player count — see [`SetupMode`].
//!
//! Seed derivation is **not** bit-compatible with the TS engine (statistical
//! parity only, per the port plan): the same Rust seed always reproduces the
//! same board within Rust, and `deck` mode picks from the same printed 12,
//! but a given seed does not select the same board as in TS.

use crate::ambitions::{AMBITION_MARKER_COUNT, AMBITION_MARKERS, AmbitionMarkerDef};
use crate::cards::{ACTION_CARD_COUNT, action_deck_for};
use crate::court::{COURT_CARD_COUNT, court_row_size};
use crate::inline_vec::InlineVec;
use crate::map::{
    CLUSTER_COUNT, SYSTEM_COUNT, SystemDef, build_systems, gate_id, planet_id, resolve_adjacency,
};
use crate::rng::{Rng, SplitMix64};
use crate::types::{ActionCardId, CourtCardId, SystemId};

/// Power that ends the game at a chapter break (p2).
pub const fn power_threshold(players: u8) -> u8 {
    match players {
        2 => 33,
        3 => 30,
        _ => 27,
    }
}

pub const MAX_CHAPTERS: u8 = 5;
pub const HAND_SIZE: u8 = 6;
pub const MIN_PLAYERS: u8 = 2;
pub const MAX_PLAYERS: u8 = 4;

/// One player's starting systems on a setup card: city + 3 ships in `a`,
/// starport + 3 ships in `b`, 2 ships in each `c` (two Cs at 2 players).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct StartPosition {
    pub a: SystemId,
    pub b: SystemId,
    pub c: InlineVec<SystemId, 2>,
}

/// A setup card: out-of-play clusters plus one start per player, in card
/// order (1A first). Which player takes which follows the initiative marker
/// — see the TS `newGame`. Serialize-only under the `serde` feature: the
/// `name` is a `&'static str` and setups are always re-derived from a seed,
/// never deserialized.
#[derive(Clone, PartialEq, Eq, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct SetupCard {
    pub name: &'static str,
    pub out_of_play: InlineVec<u8, 2>,
    pub starts: InlineVec<StartPosition, 4>,
}

/// The printed setup-card deck in transcription-friendly form; use
/// [`setup_deck`] / [`draw_setup`] for the runtime shape.
struct PrintedSetup {
    name: &'static str,
    out_of_play: &'static [u8],
    starts: &'static [(u8, u8, &'static [u8])],
}

const PRINTED_SETUPS_2P: [PrintedSetup; 4] = [
    PrintedSetup {
        name: "Mix Up 1",
        out_of_play: &[1, 4],
        starts: &[(14, 10, &[0, 21]), (23, 11, &[2, 12])],
    },
    PrintedSetup {
        name: "Mix Up 2",
        out_of_play: &[0, 3],
        starts: &[(18, 5, &[22, 8]), (6, 21, &[16, 11])],
    },
    PrintedSetup {
        name: "Homelands",
        out_of_play: &[0, 3],
        starts: &[(17, 21, &[19, 16]), (11, 9, &[5, 8])],
    },
    // Marked "for experienced players".
    PrintedSetup {
        name: "Frontiers",
        out_of_play: &[0, 5],
        starts: &[(19, 15, &[8, 11]), (9, 17, &[16, 13])],
    },
];

const PRINTED_SETUPS_3P: [PrintedSetup; 4] = [
    PrintedSetup {
        name: "Mix Up",
        out_of_play: &[0, 3],
        starts: &[(11, 18, &[4]), (19, 5, &[8]), (7, 9, &[16])],
    },
    PrintedSetup {
        name: "Homelands",
        out_of_play: &[4, 5],
        starts: &[(7, 10, &[8]), (3, 5, &[4]), (1, 15, &[12])],
    },
    PrintedSetup {
        name: "Frontiers",
        out_of_play: &[1, 2],
        starts: &[(3, 15, &[20]), (19, 2, &[16]), (14, 21, &[0])],
    },
    // Marked "for experienced players".
    PrintedSetup {
        name: "Core Conflict",
        out_of_play: &[2, 5],
        starts: &[(3, 6, &[0]), (7, 2, &[4]), (1, 5, &[12])],
    },
];

const PRINTED_SETUPS_4P: [PrintedSetup; 4] = [
    PrintedSetup {
        name: "Mix Up 1",
        out_of_play: &[2],
        starts: &[
            (13, 23, &[0]),
            (15, 19, &[20]),
            (17, 3, &[12]),
            (21, 1, &[16]),
        ],
    },
    PrintedSetup {
        name: "Mix Up 2",
        out_of_play: &[3],
        starts: &[(19, 9, &[4]), (11, 18, &[0]), (7, 3, &[8]), (1, 5, &[16])],
    },
    PrintedSetup {
        name: "Mix Up 3",
        out_of_play: &[5],
        starts: &[(11, 18, &[0]), (1, 9, &[4]), (3, 15, &[8]), (13, 6, &[16])],
    },
    PrintedSetup {
        name: "Frontiers",
        out_of_play: &[4],
        starts: &[(3, 10, &[4]), (7, 23, &[8]), (14, 5, &[20]), (1, 21, &[12])],
    },
];

fn printed_setups(players: u8) -> &'static [PrintedSetup; 4] {
    match players {
        2 => &PRINTED_SETUPS_2P,
        3 => &PRINTED_SETUPS_3P,
        _ => &PRINTED_SETUPS_4P,
    }
}

fn to_setup_card(printed: &PrintedSetup) -> SetupCard {
    let mut starts = InlineVec::new();
    for &(a, b, c) in printed.starts {
        starts.push(StartPosition {
            a: SystemId(a),
            b: SystemId(b),
            c: c.iter().map(|&s| SystemId(s)).collect(),
        });
    }
    SetupCard {
        name: printed.name,
        out_of_play: InlineVec::from_slice(printed.out_of_play),
        starts,
    }
}

/// The printed 12: 4 setup cards per player count (p4 step I).
pub fn setup_deck(players: u8) -> [SetupCard; 4] {
    let printed = printed_setups(players);
    core::array::from_fn(|i| to_setup_card(&printed[i]))
}

/// How a game's opening is chosen: `Deck` draws one of the printed cards for
/// the player count (the game as played), `Draw` invents a fresh legal
/// opening per seed (for measurement batches — see the TS source for why).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum SetupMode {
    #[default]
    Deck,
    Draw,
}

// Domain-separation constants so the setup draw never disturbs (and is never
// disturbed by) any other stream derived from the same seed.
const DECK_STREAM: u64 = 0x5EED_DECC;
const DRAW_STREAM: u64 = 0x5EED_D4A0;

/// Draw the setup for `seed` (p4 step I). The same seed always reproduces
/// the same board — which is what lets `make_variant` and `new_game` (R1)
/// derive it independently and still agree.
pub fn draw_setup(players: u8, seed: u64, mode: SetupMode) -> SetupCard {
    match mode {
        SetupMode::Draw => generate_setup(players, seed),
        SetupMode::Deck => {
            let mut rng = SplitMix64::new(seed ^ DECK_STREAM);
            // Burn one output so seed and state decorrelate even for tiny
            // seeds (SplitMix64's first output of seed 0 is fine, but cheap
            // insurance costs one add+xor).
            let index = {
                rng.next_u64();
                rng.gen_range(4)
            };
            to_setup_card(&printed_setups(players)[index])
        }
    }
}

/// Draw a random legal setup for `players`, determined entirely by `seed`.
///
/// Every stated setup rule is obeyed (p4 step J, p5 steps N-O):
///
/// - 1 cluster out of play at 4 players, 2 at 2-3;
/// - one A, one B and one C system per player, two Cs at 2 players;
/// - A and B are planets (they take a city and a starport, and their printed
///   resource is gained at step O);
/// - nothing starts in an out-of-play cluster, and no system is shared.
pub fn generate_setup(players: u8, seed: u64) -> SetupCard {
    // A private stream, so drawing a setup never disturbs the game's RNG.
    let mut rng = SplitMix64::new(seed ^ DRAW_STREAM);
    rng.next_u64();

    let removed = if players == 4 { 1 } else { 2 };
    let mut order: [u8; CLUSTER_COUNT] = core::array::from_fn(|c| c as u8);
    rng.shuffle(&mut order);
    let mut out_of_play = InlineVec::<u8, 2>::from_slice(&order[..removed]);
    out_of_play.as_mut_slice().sort_unstable();
    let live = &order[removed..];

    let mut claimed = [false; SYSTEM_COUNT];

    // Draw an unclaimed system, preferring the given clusters but never
    // returning one already taken; fall back to "anywhere live" so the
    // no-sharing rule stays exact (a player's B planet may spread into a
    // neighbouring cluster and take a planet a later player wanted).
    let mut take = |prefer: &[u8], gate: bool, rng: &mut SplitMix64| -> SystemId {
        let of = |clusters: &[u8], claimed: &[bool; SYSTEM_COUNT]| -> Vec<SystemId> {
            clusters
                .iter()
                .flat_map(|&c| {
                    if gate {
                        vec![gate_id(c)]
                    } else {
                        (0..3).map(|p| planet_id(c, p)).collect()
                    }
                })
                .filter(|s| !claimed[s.as_index()])
                .collect()
        };
        let free = of(prefer, &claimed);
        let candidates = if free.is_empty() {
            of(live, &claimed)
        } else {
            free
        };
        // The board always has room: 12-15 live planets and 4-5 live gates
        // against at most 8 planets and 4 gates needed.
        let chosen = *rng.pick(&candidates);
        claimed[chosen.as_index()] = true;
        chosen
    };

    let mut starts = InlineVec::new();
    let n = CLUSTER_COUNT as u8;
    for p in 0..players as usize {
        // Each player homes in a distinct live cluster.
        let home = live[p % live.len()];
        let mut neighbours = InlineVec::<u8, 2>::new();
        for c in [(home + 1) % n, (home + n - 1) % n] {
            if live.contains(&c) {
                neighbours.push(c);
            }
        }

        // A sits in the home cluster. B often sits in a neighbouring one:
        // the printed cards spread a player's two planets rather than
        // stacking them, which is what makes the opening moves matter.
        let a = take(&[home], false, &mut rng);
        let spread = !neighbours.is_empty() && rng.next_f64() < 0.5;
        let b = if spread {
            take(neighbours.as_slice(), false, &mut rng)
        } else {
            take(&[home], false, &mut rng)
        };

        let mut c = InlineVec::new();
        c.push(take(&[home], true, &mut rng));
        if players == 2 {
            c.push(take(neighbours.as_slice(), true, &mut rng));
        }
        starts.push(StartPosition { a, b, c });
    }

    SetupCard {
        name: "Draw",
        out_of_play,
        starts,
    }
}

/// A game variant = player count + map + decks + thresholds, all plain data.
#[derive(Clone, PartialEq, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct VariantDef {
    pub players: u8,
    /// The 24 systems with adjacency already resolved for this game's
    /// out-of-play clusters.
    pub systems: [SystemDef; SYSTEM_COUNT],
    pub action_deck: InlineVec<ActionCardId, ACTION_CARD_COUNT>,
    pub court_deck: [CourtCardId; COURT_CARD_COUNT],
    pub ambition_markers: [AmbitionMarkerDef; AMBITION_MARKER_COUNT],
    /// Face-up Court row width: 3 at 2 players, 4 at 3-4.
    pub court_row_size: u8,
    /// Power that ends the game at a chapter break.
    pub power_threshold: u8,
    pub max_chapters: u8,
    pub hand_size: u8,
    /// Model p17's "when you gain a resource you may rearrange slots" as a
    /// decision instead of filling the leftmost open slot.
    ///
    /// Off by default: it adds an [`crate::Action::PlaceResource`] variant the
    /// TS engine has no counterpart for, so a run that must stay comparable to
    /// the TS gauntlet ledger leaves it off. On, the holder chooses the
    /// raid-cost tier each gained token sits in, which is the only thing the
    /// slot index affects (`RAID_COSTS = [1,1,2,2,3,3]`).
    pub choose_placement: bool,
}

/// Build a variant. `setup_index` seeds the opening — see [`draw_setup`].
/// The same value always yields the same board; pass the same `mode` to
/// every derivation of the game or they will disagree about which clusters
/// are out of play.
pub fn make_variant(players: u8, setup_index: u64, mode: SetupMode) -> VariantDef {
    debug_assert!((MIN_PLAYERS..=MAX_PLAYERS).contains(&players));
    let setup = draw_setup(players, setup_index, mode);
    VariantDef {
        players,
        systems: resolve_adjacency(&build_systems(), setup.out_of_play.as_slice()),
        action_deck: action_deck_for(players),
        court_deck: core::array::from_fn(|i| CourtCardId(i as u8)),
        ambition_markers: AMBITION_MARKERS,
        court_row_size: court_row_size(players),
        power_threshold: power_threshold(players),
        max_chapters: MAX_CHAPTERS,
        hand_size: HAND_SIZE,
        choose_placement: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::{cluster_of, is_gate};

    #[test]
    fn deck_mode_picks_a_printed_card() {
        for players in 2..=4u8 {
            let printed = setup_deck(players);
            for seed in 0..64 {
                let card = draw_setup(players, seed, SetupMode::Deck);
                assert!(printed.contains(&card), "seed {seed} not a printed card");
                // Same seed, same card.
                assert_eq!(card, draw_setup(players, seed, SetupMode::Deck));
            }
        }
    }

    #[test]
    fn deck_mode_reaches_all_four_cards() {
        for players in 2..=4u8 {
            let printed = setup_deck(players);
            let mut seen = [false; 4];
            for seed in 0..64 {
                let card = draw_setup(players, seed, SetupMode::Deck);
                seen[printed.iter().position(|c| *c == card).unwrap()] = true;
            }
            assert!(seen.iter().all(|&s| s), "{players}p missed a printed card");
        }
    }

    #[test]
    fn generated_setups_are_legal() {
        for players in 2..=4u8 {
            for seed in 0..500u64 {
                let card = generate_setup(players, seed);
                assert_eq!(card, generate_setup(players, seed), "seed {seed} unstable");

                let expected_removed = if players == 4 { 1 } else { 2 };
                assert_eq!(card.out_of_play.len(), expected_removed);
                assert!(card.out_of_play.as_slice().is_sorted());
                assert_eq!(card.starts.len(), players as usize);

                let dead = |s: SystemId| card.out_of_play.contains(&cluster_of(s));
                let mut all: Vec<SystemId> = Vec::new();
                for start in card.starts.iter() {
                    assert!(!is_gate(start.a), "A must be a planet");
                    assert!(!is_gate(start.b), "B must be a planet");
                    assert_eq!(card.starts.len(), players as usize);
                    assert_eq!(start.c.len(), if players == 2 { 2 } else { 1 });
                    for &c in start.c.iter() {
                        assert!(is_gate(c), "C must be a gate");
                    }
                    all.push(start.a);
                    all.push(start.b);
                    all.extend(start.c.iter().copied());
                }
                for &s in &all {
                    assert!(!dead(s), "seed {seed}: start in out-of-play cluster");
                }
                let mut deduped = all.clone();
                deduped.sort_unstable();
                deduped.dedup();
                assert_eq!(deduped.len(), all.len(), "seed {seed}: shared system");
            }
        }
    }

    #[test]
    fn make_variant_is_deterministic_and_resolved() {
        for players in 2..=4u8 {
            for mode in [SetupMode::Deck, SetupMode::Draw] {
                let a = make_variant(players, 17, mode);
                let b = make_variant(players, 17, mode);
                assert_eq!(a, b);
                assert_eq!(a.court_row_size, court_row_size(players));
                assert_eq!(a.power_threshold, power_threshold(players));
                // Adjacency was resolved: the drawn card's dead clusters
                // have no edges.
                let setup = draw_setup(players, 17, mode);
                for s in &a.systems {
                    if setup.out_of_play.contains(&s.cluster) {
                        assert!(s.adjacent.is_empty());
                    }
                }
            }
        }
    }

    #[test]
    fn thresholds_match_ts() {
        assert_eq!(power_threshold(2), 33);
        assert_eq!(power_threshold(3), 30);
        assert_eq!(power_threshold(4), 27);
        assert_eq!(MAX_CHAPTERS, 5);
        assert_eq!(HAND_SIZE, 6);
    }
}
