//! Random self-play fuzzing (plan §5/§6, R2 verification): uniform play
//! over the legal list, seeded, across player counts and both setup modes,
//! with conservation invariants checked at every decision:
//!
//! - termination under a 200k-decision guard, `legal_actions` never empty,
//!   `apply_action_mut` never errors on an emitted action;
//! - piece conservation per player: ships (map + supply + trophies = 15),
//!   starports (map + supply + trophies = 5), cities (map + trophies =
//!   `cities_used` <= 5), agents (supply + Court + captives + trophies +
//!   at most one Outrage debit per flipped marker = 10);
//! - resource-token conservation per type against the post-setup total
//!   (supply + held + mid-Prelude spends; the 2p phantom is deducted at
//!   setup);
//! - the action-card multiset (deck + discard + hands + played) and the
//!   Court-card multiset (row + deck + discard + held) never change.
//!
//! The default run covers 500 seeds; the full sweep (10k seeds x {2,3,4}
//! players x both modes = 60k games) is `#[ignore]`d:
//!
//! ```sh
//! cargo test --release --test fuzz -- --ignored --nocapture
//! ```

use arcs_engine::cards::ACTION_CARD_COUNT;
use arcs_engine::court::COURT_CARD_COUNT;
use arcs_engine::state::TrophyKind;
use arcs_engine::{
    Action, BuildingKind, GameState, Pending, Phase, ResourceType, Rng, SetupMode, SplitMix64,
    VariantDef, apply_action_mut, get_pending, legal_actions, make_variant, new_game,
    resolve_chance_mut,
};

const DECISION_GUARD: u32 = 200_000;

/// Totals fixed at setup that every later state must still account for.
struct Baseline {
    resources: [u16; ResourceType::COUNT],
    action_cards: [u8; ACTION_CARD_COUNT],
}

fn baseline(s: &GameState) -> Baseline {
    let mut resources = [0u16; ResourceType::COUNT];
    for (r, total) in resources.iter_mut().enumerate() {
        *total = s.supply[r] as u16;
    }
    for p in 0..s.players as usize {
        for r in s.player_states[p].resources.iter().flatten() {
            resources[r.as_index()] += 1;
        }
    }
    let mut action_cards = [0u8; ACTION_CARD_COUNT];
    for &c in s.action_deck.iter() {
        action_cards[c.as_index()] += 1;
    }
    Baseline {
        resources,
        action_cards,
    }
}

fn check_invariants(s: &GameState, base: &Baseline, ctx: &str) {
    let players = s.players as usize;

    // --- pieces per player ---
    for p in 0..players {
        let ps = &s.player_states[p];

        let ships_on_map: u16 = s
            .systems
            .iter()
            .map(|sys| (sys.fresh[p] + sys.damaged[p]) as u16)
            .sum();
        let ship_trophies: u16 = s
            .player_states
            .iter()
            .map(|o| o.trophies[p][TrophyKind::Ship.as_index()] as u16)
            .sum();
        assert_eq!(
            ships_on_map + ps.ships_supply as u16 + ship_trophies,
            15,
            "{ctx}: player {p} ships not conserved"
        );

        let mut starports_on_map = 0u16;
        let mut cities_on_map = 0u16;
        for sys in &s.systems {
            for b in sys.buildings.iter() {
                if b.player().as_index() != p {
                    continue;
                }
                match b.kind() {
                    BuildingKind::Starport => starports_on_map += 1,
                    BuildingKind::City => cities_on_map += 1,
                }
            }
        }
        let starport_trophies: u16 = s
            .player_states
            .iter()
            .map(|o| o.trophies[p][TrophyKind::Starport.as_index()] as u16)
            .sum();
        assert_eq!(
            starports_on_map + ps.starports_supply as u16 + starport_trophies,
            5,
            "{ctx}: player {p} starports not conserved"
        );

        let city_trophies: u16 = s
            .player_states
            .iter()
            .map(|o| o.trophies[p][TrophyKind::City.as_index()] as u16)
            .sum();
        assert!(ps.cities_used <= 5, "{ctx}: player {p} citiesUsed > 5");
        assert_eq!(
            cities_on_map + city_trophies,
            ps.cities_used as u16,
            "{ctx}: player {p} cities not conserved"
        );

        let agents_on_court: u16 = s.court.iter().map(|slot| slot.agents[p] as u16).sum();
        let agents_captive: u16 = s.player_states.iter().map(|o| o.captives[p] as u16).sum();
        let agent_trophies: u16 = s
            .player_states
            .iter()
            .map(|o| o.trophies[p][TrophyKind::Agent.as_index()] as u16)
            .sum();
        let accounted = ps.agents_supply as u16 + agents_on_court + agents_captive + agent_trophies;
        // Each flipped Outrage marker consumed at most one agent (none when
        // the supply was empty at the flip), so the shortfall is bounded by
        // the flag count.
        let outraged = ps.outrage.iter().filter(|&&o| o).count() as u16;
        assert!(
            accounted <= 10 && 10 - accounted <= outraged,
            "{ctx}: player {p} agents not conserved ({accounted} accounted, {outraged} outrage flags)"
        );
    }

    // --- resource tokens per type ---
    let mut resources = [0u16; ResourceType::COUNT];
    for (r, total) in resources.iter_mut().enumerate() {
        *total = s.supply[r] as u16 + s.cartel[r] as u16;
    }
    for p in 0..players {
        for r in s.player_states[p].resources.iter().flatten() {
            resources[r.as_index()] += 1;
        }
    }
    if let Some(turn) = &s.turn {
        // Tokens spent this Prelude sit off-board until the Prelude ends.
        for (r, &n) in turn.prelude_spent.iter().enumerate() {
            resources[r] += n as u16;
        }
    }
    assert_eq!(
        resources, base.resources,
        "{ctx}: resource tokens not conserved"
    );

    // --- the action-card multiset ---
    let mut cards = [0u8; ACTION_CARD_COUNT];
    for &c in s.action_deck.iter() {
        cards[c.as_index()] += 1;
    }
    for &c in s.action_discard.iter() {
        cards[c.as_index()] += 1;
    }
    for p in 0..players {
        for &c in s.player_states[p].hand.iter() {
            cards[c.as_index()] += 1;
        }
    }
    for c in s.round.played.iter() {
        cards[c.card.as_index()] += 1;
    }
    assert_eq!(
        cards, base.action_cards,
        "{ctx}: action cards not conserved"
    );

    // --- the Court-card multiset: every card exactly once ---
    let mut court = [0u8; COURT_CARD_COUNT];
    for slot in s.court.iter() {
        court[slot.card.as_index()] += 1;
    }
    for &c in s.court_deck.iter() {
        court[c.as_index()] += 1;
    }
    for &c in s.court_discard.iter() {
        court[c.as_index()] += 1;
    }
    for p in 0..players {
        for &c in s.player_states[p].guild_cards.iter() {
            court[c.as_index()] += 1;
        }
    }
    // R3: a secured Vox card sits in limbo while its decision is pending,
    // and an attached Union card sits on a played action card.
    if let Some(pending) = s.pending_vox {
        court[pending.card.as_index()] += 1;
    }
    for u in s.unions.iter() {
        court[u.card.as_index()] += 1;
    }
    assert_eq!(
        court, [1u8; COURT_CARD_COUNT],
        "{ctx}: court cards not conserved"
    );

    // --- nothing in an out-of-play cluster ---
    for (i, sys) in s.systems.iter().enumerate() {
        if !sys.out_of_play {
            continue;
        }
        assert!(
            sys.fresh.iter().all(|&n| n == 0)
                && sys.damaged.iter().all(|&n| n == 0)
                && sys.buildings.is_empty(),
            "{ctx}: pieces in out-of-play system {i}"
        );
    }
}

/// Play one full random game, checking invariants at every decision.
fn fuzz_one(players: u8, seed: u64, mode: SetupMode) {
    let ctx = format!("{players}p seed {seed} {mode:?}");
    let v: VariantDef = make_variant(players, seed, mode);
    let mut rng = SplitMix64::new(seed.wrapping_mul(0x9E3779B97F4A7C15).wrapping_add(1));
    let mut s = new_game(&v, &mut rng, seed, mode);
    let base = baseline(&s);
    check_invariants(&s, &base, &ctx);

    let mut acts: Vec<Action> = Vec::new();
    let mut decisions = 0u32;
    loop {
        match get_pending(&s, &v) {
            Pending::Over => break,
            Pending::Chance => {
                resolve_chance_mut(&mut s, &v, &mut rng)
                    .unwrap_or_else(|e| panic!("{ctx}: chance failed: {e}"));
            }
            Pending::Decision { .. } => {
                legal_actions(&s, &v, &mut acts);
                assert!(
                    !acts.is_empty(),
                    "{ctx}: no legal actions in phase {:?}",
                    s.phase
                );
                let a = acts[rng.gen_range(acts.len())];
                apply_action_mut(&mut s, &v, a)
                    .unwrap_or_else(|e| panic!("{ctx}: apply {a:?} failed: {e}"));
                decisions += 1;
                assert!(
                    decisions < DECISION_GUARD,
                    "{ctx}: game did not end within {DECISION_GUARD} decisions"
                );
                check_invariants(&s, &base, &ctx);
            }
        }
    }
    assert_eq!(s.phase, Phase::Over, "{ctx}: ended outside Over");
}

/// The default-suite sweep: 500 seeds cycled across {2,3,4} players and
/// both setup modes.
#[test]
fn fuzz_random_self_play_500_seeds() {
    for seed in 0..500u64 {
        let players = 2 + (seed % 3) as u8;
        let mode = if (seed / 3) % 2 == 0 {
            SetupMode::Deck
        } else {
            SetupMode::Draw
        };
        fuzz_one(players, seed, mode);
    }
}

/// The full sweep the plan calls for: 10k seeds x every player count x both
/// setup modes (60k games). Run with
/// `cargo test --release --test fuzz -- --ignored --nocapture`.
#[test]
#[ignore = "long fuzz sweep: run with --release -- --ignored"]
fn fuzz_random_self_play_10k_seeds() {
    let mut games = 0u64;
    for seed in 0..10_000u64 {
        for players in 2..=4u8 {
            for mode in [SetupMode::Deck, SetupMode::Draw] {
                fuzz_one(players, seed, mode);
                games += 1;
            }
        }
        if (seed + 1) % 1000 == 0 {
            println!("fuzz: {} seeds, {games} games clean", seed + 1);
        }
    }
    println!("fuzz: 10000 seeds, {games} games clean");
}
