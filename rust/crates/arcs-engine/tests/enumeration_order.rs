//! `legal_actions` enumerates in a pinned order.
//!
//! Agents answer a decision with an index into the legal list, so the order is
//! part of the engine's public contract — see the doc comment on
//! `legal_actions`. Recorded trajectories, cross-language policies and cached
//! decisions all read that index, and none of them can tell that a refactor
//! reordered the enumerator underneath them.
//!
//! These fixtures are the tripwire: a handful of fixed seeded states with
//! their full legal sequence written out as `encode_action` keys. If a change
//! to the enumerator is intended, regenerate them with
//!
//! ```text
//! cargo test -p arcs-engine --test enumeration_order -- --ignored --nocapture
//! ```
//!
//! and read the diff before pasting it in — every line that moves is an index
//! whose meaning changed.

use arcs_engine::{
    Action, Pending, Player, Rng, SetupMode, SplitMix64, apply_action_mut, encode_action,
    get_pending, legal_actions, make_variant, new_game, resolve_chance_mut,
};

/// Walk a seeded game with a fixed uniform-random driver and return the legal
/// list at decision node `stop`, as canonical keys.
///
/// The driver is part of the fixture: same seed, same sequence of chosen
/// actions, so the position reached at node `stop` is fully determined.
fn legal_keys_at(players: u8, seed: u64, stop: usize) -> (Player, Vec<String>) {
    let v = make_variant(players, seed, SetupMode::Draw);
    let mut rng = SplitMix64::new(seed);
    let mut s = new_game(&v, &mut rng, seed, SetupMode::Draw);
    let mut legal: Vec<Action> = Vec::new();
    let mut node = 0usize;
    loop {
        match get_pending(&s, &v) {
            Pending::Over => panic!("game ended before decision {stop}"),
            Pending::Chance => resolve_chance_mut(&mut s, &v, &mut rng).unwrap(),
            Pending::Decision { player } => {
                legal_actions(&s, &v, &mut legal);
                if node == stop {
                    return (player, legal.iter().map(|a| encode_action(*a)).collect());
                }
                node += 1;
                let pick = legal[rng.gen_range(legal.len())];
                apply_action_mut(&mut s, &v, pick).unwrap();
            }
        }
    }
}

/// `(players, seed, decision index, seat, keys)`.
type Fixture = (u8, u64, usize, u8, &'static [&'static str]);

/// Pinned enumeration order. Regenerate with `print_enumeration_fixtures`.
const FIXTURES: &[Fixture] = &[
    // The opening lead: hand order, then `pi`.
    (
        3,
        1,
        0,
        1,
        &["ld:9", "ld:8", "ld:3", "ld:24", "ld:12", "ld:19", "pi"],
    ),
    // A Prelude: spendable resources, then "begin actions".
    (3, 1, 3, 1, &["sr:1", "ba"]),
    (3, 7, 12, 2, &["ba"]),
    (4, 3, 20, 3, &["ba"]),
    // The wide case, and the one that matters most: an Actions node where
    // moves are enumerated source-major then destination then ship count,
    // followed by influence, then every battle dice split, then `et`.
    (
        2,
        5,
        30,
        1,
        &[
            "mv:8:12:1",
            "mv:8:12:2",
            "mv:8:9:1",
            "mv:8:9:2",
            "mv:8:10:1",
            "mv:8:10:2",
            "mv:8:11:1",
            "mv:8:11:2",
            "mv:8:20:1",
            "mv:8:20:2",
            "mv:11:8:1",
            "mv:11:8:2",
            "mv:11:8:3",
            "mv:11:10:1",
            "mv:11:10:2",
            "mv:11:10:3",
            "mv:12:8:1",
            "mv:12:16:1",
            "mv:12:13:1",
            "mv:12:14:1",
            "mv:12:15:1",
            "mv:13:12:1",
            "mv:13:14:1",
            "mv:14:12:1",
            "mv:14:12:2",
            "mv:14:12:3",
            "mv:14:13:1",
            "mv:14:13:2",
            "mv:14:13:3",
            "mv:14:15:1",
            "mv:14:15:2",
            "mv:14:15:3",
            "in:0",
            "in:1",
            "in:2",
            "bt:14:0:0/1/0",
            "bt:14:0:0/2/0",
            "bt:14:0:0/3/0",
            "bt:14:0:1/0/0",
            "bt:14:0:1/1/0",
            "bt:14:0:1/2/0",
            "bt:14:0:2/0/0",
            "bt:14:0:2/1/0",
            "bt:14:0:3/0/0",
            "et",
        ],
    ),
];

#[test]
fn legal_action_order_is_pinned() {
    for &(players, seed, stop, seat, expected) in FIXTURES {
        let (player, keys) = legal_keys_at(players, seed, stop);
        assert_eq!(
            player,
            Player(seat),
            "{players}p seed {seed} decision {stop}: the seat to move moved"
        );
        assert_eq!(
            keys, expected,
            "{players}p seed {seed} decision {stop}: the enumeration order changed. \
             Every consumer that stores an index into this list — trajectories, \
             external policies, cached decisions — now means something else."
        );
    }
}

/// Regenerate the fixtures above.
#[test]
#[ignore = "fixture generator"]
fn print_enumeration_fixtures() {
    for &(players, seed, stop, _, _) in FIXTURES {
        let (player, keys) = legal_keys_at(players, seed, stop);
        let list = keys
            .iter()
            .map(|k| format!("{k:?}"))
            .collect::<Vec<_>>()
            .join(", ");
        println!("({players}, {seed}, {stop}, {}, &[{list}]),", player.0);
    }
}
