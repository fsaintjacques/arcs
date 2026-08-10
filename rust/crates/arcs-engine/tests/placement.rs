//! p17 resource-slot placement as a decision (`VariantDef::choose_placement`).
//!
//! The rule is "when you gain a resource you may rearrange slots", and the
//! slot index affects exactly one thing: the keys a raider spends to steal it
//! (`RAID_COSTS = [1,1,2,2,3,3]`). These tests pin the encoding that keeps
//! that choice cheap — tiers, not slots, one token at a time.

mod common;

use arcs_engine::player_board::raid_cost;
use arcs_engine::state::PlayerState;
use arcs_engine::{
    Action, Pending, Player, ResourceType, Rng, SetupMode, SplitMix64, VariantDef, get_pending,
    legal_actions, make_variant, new_game,
};

use ResourceType::{Fuel, Material, Relic};

fn placement_variant(players: u8, seed: u64) -> VariantDef {
    let mut v = make_variant(players, seed, SetupMode::Draw);
    v.choose_placement = true;
    v
}

/// The whole cardinality argument in one assertion: 6 slots, but never more
/// than 3 options, because the only observable difference between two slots
/// is their raid cost.
#[test]
fn branching_is_bounded_by_the_three_raid_cost_tiers() {
    let mut p = PlayerState::new();
    p.cities_used = 5; // all 6 slots open
    assert_eq!(p.open_resource_slots(), 6);
    assert_eq!(p.placement_tiers().as_slice(), &[1, 2, 3]);

    // Filling one slot of a tier does not remove the tier while its twin is
    // free; filling both does.
    p.resources[0] = Some(Material);
    assert_eq!(p.placement_tiers().as_slice(), &[1, 2, 3]);
    p.resources[1] = Some(Material);
    assert_eq!(p.placement_tiers().as_slice(), &[2, 3]);
    p.resources[2] = Some(Fuel);
    p.resources[3] = Some(Fuel);
    assert_eq!(p.placement_tiers().as_slice(), &[3]);
    p.resources[4] = Some(Relic);
    p.resources[5] = Some(Relic);
    assert!(p.placement_tiers().is_empty());
}

/// Three open slots is the opening position: tiers 1 and 2 only.
#[test]
fn a_narrow_mat_offers_fewer_tiers() {
    let p = PlayerState::new();
    assert_eq!(p.open_resource_slots(), 3);
    assert_eq!(p.placement_tiers().as_slice(), &[1, 2]);
}

/// A queued token is placed in the named tier's cheapest free slot — within a
/// tier the slots are indistinguishable, so the choice stops at the tier.
#[test]
fn placing_lands_in_the_named_tier() {
    let mut p = PlayerState::new();
    p.cities_used = 5;
    assert!(p.queue_resource(Relic));
    assert!(p.place_queued(3), "tier 3 is free");
    assert_eq!(p.resources[4], Some(Relic));
    assert_eq!(raid_cost(4), 3);

    assert!(p.queue_resource(Material));
    assert!(p.place_queued(3), "tier 3 has a second slot");
    assert_eq!(p.resources[5], Some(Material));

    assert!(p.queue_resource(Fuel));
    assert!(!p.place_queued(3), "tier 3 is now full");
    assert!(p.place_queued(1), "tier 1 is free");
    assert_eq!(p.resources[0], Some(Fuel));
}

/// "Is there room?" must not depend on which slot is picked, or a caller that
/// gains several tokens would promise room it does not have.
#[test]
fn queued_tokens_reserve_their_slots() {
    let mut p = PlayerState::new(); // 3 open slots
    assert_eq!(p.free_slots(), 3);
    assert!(p.queue_resource(Material));
    assert_eq!(p.free_slots(), 2);
    assert!(p.queue_resource(Fuel));
    assert!(p.queue_resource(Relic));
    assert_eq!(p.free_slots(), 0);
    assert!(!p.queue_resource(Material), "a full mat cannot hold more");
}

/// The placement decision outranks the phase, so no other rule ever observes
/// a mat with tokens still in hand.
#[test]
fn setup_tokens_are_placed_before_the_first_deal() {
    let v = placement_variant(3, 7);
    let mut rng = SplitMix64::new(7);
    let s = new_game(&v, &mut rng, 7, SetupMode::Draw);

    // Every seat drew 2 starting resources (p5 step O) and none are placed.
    for p in 0..3 {
        assert_eq!(s.player(Player(p)).pending_resources.len(), 2);
        assert!(s.player(Player(p)).held_resources().is_empty());
    }

    let pending = get_pending(&s, &v);
    assert_eq!(pending, Pending::Decision { player: Player(0) });
    let mut legal = Vec::new();
    legal_actions(&s, &v, &mut legal);
    assert_eq!(
        legal,
        vec![
            Action::PlaceResource { tier: 1 },
            Action::PlaceResource { tier: 2 }
        ]
    );
}

/// With the flag off, nothing changes: no queue, no extra decisions.
#[test]
fn the_flag_is_off_by_default_and_inert() {
    let v = make_variant(3, 7, SetupMode::Draw);
    assert!(!v.choose_placement);
    let mut rng = SplitMix64::new(7);
    let s = new_game(&v, &mut rng, 7, SetupMode::Draw);
    for p in 0..3 {
        assert!(s.player(Player(p)).pending_resources.is_empty());
        assert_eq!(s.player(Player(p)).held_resources().len(), 2);
        // Leftmost fill, as before: the cheapest slots.
        assert!(s.player(Player(p)).resources[0].is_some());
        assert!(s.player(Player(p)).resources[1].is_some());
    }
    assert_eq!(get_pending(&s, &v), Pending::Chance);
}

/// What the choice costs, printed rather than asserted. Run with:
///
/// ```text
/// cargo test -p arcs-engine --release --test placement -- --ignored --nocapture
/// ```
#[test]
#[ignore]
fn placement_cost_report() {
    fn run(choose: bool) -> (f64, f64, f64) {
        let (mut decisions, mut placements, mut options) = (0u64, 0u64, 0u64);
        let games = 300u64;
        for seed in 0..games {
            let mut v = make_variant(3, seed, SetupMode::Draw);
            v.choose_placement = choose;
            let mut rng = SplitMix64::new(seed ^ 0xA11CE);
            let mut s = new_game(&v, &mut rng, seed, SetupMode::Draw);
            let mut legal = Vec::new();
            for _ in 0..200_000 {
                match get_pending(&s, &v) {
                    Pending::Over => break,
                    Pending::Chance => {
                        arcs_engine::resolve_chance_mut(&mut s, &v, &mut rng).expect("chance");
                    }
                    Pending::Decision { .. } => {
                        legal_actions(&s, &v, &mut legal);
                        if legal.is_empty() {
                            break;
                        }
                        decisions += 1;
                        if matches!(legal[0], Action::PlaceResource { .. }) {
                            placements += 1;
                            options += legal.len() as u64;
                        }
                        let pick = (rng.next_u64() % legal.len() as u64) as usize;
                        arcs_engine::apply_action_mut(&mut s, &v, legal[pick]).expect("legal");
                    }
                }
            }
        }
        (
            decisions as f64 / games as f64,
            placements as f64 / games as f64,
            if placements > 0 {
                options as f64 / placements as f64
            } else {
                0.0
            },
        )
    }

    let (off, _, _) = run(false);
    let (on, placements, mean_options) = run(true);
    println!("\n-- 300 random 3p games --");
    println!("decisions/game, placement off: {off:.1}");
    println!("decisions/game, placement on:  {on:.1}");
    println!("  of which placements:         {placements:.1}");
    println!("  mean options per placement:  {mean_options:.3}  (hard ceiling 3)");
    println!(
        "decision overhead:             {:+.1}%",
        100.0 * (on / off - 1.0)
    );
}

/// A full game under the flag: every decision stays legal and the queue always
/// drains, so the game terminates.
#[test]
fn a_full_game_plays_out_with_placement_on() {
    for seed in 0..25u64 {
        let v = placement_variant(3, seed);
        let mut rng = SplitMix64::new(seed ^ 0xA11CE);
        let mut s = new_game(&v, &mut rng, seed, SetupMode::Draw);
        let mut legal = Vec::new();
        let mut placements = 0usize;

        for _ in 0..200_000 {
            match get_pending(&s, &v) {
                Pending::Over => break,
                Pending::Chance => {
                    arcs_engine::resolve_chance_mut(&mut s, &v, &mut rng).expect("chance resolves");
                }
                Pending::Decision { player } => {
                    legal_actions(&s, &v, &mut legal);
                    assert!(!legal.is_empty(), "a decision with no legal action");
                    if let Action::PlaceResource { .. } = legal[0] {
                        assert!(
                            legal.len() <= 3,
                            "placement offered {} options",
                            legal.len()
                        );
                        assert!(!s.player(player).pending_resources.is_empty());
                        placements += 1;
                    }
                    let pick = (rng.next_u64() % legal.len() as u64) as usize;
                    arcs_engine::apply_action_mut(&mut s, &v, legal[pick]).expect("legal applies");
                }
            }
        }
        assert!(placements > 0, "seed {seed} never placed a resource");
        // Nothing is left in hand when the game ends.
        for p in 0..3 {
            assert!(s.player(Player(p)).pending_resources.is_empty());
        }
    }
}
