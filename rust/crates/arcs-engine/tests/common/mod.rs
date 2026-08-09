//! Test fixtures, mirroring `tests/helpers.ts` on the TS side.
#![allow(dead_code)] // each integration test binary uses a different subset

use arcs_engine::game::{apply_action_mut, get_pending, legal_actions, resolve_chance_mut};
use arcs_engine::{
    Action, ActionCardId, GameState, Pending, Phase, Player, SetupMode, SplitMix64, VariantDef,
    make_variant, new_game,
};

pub struct Fixture {
    pub v: VariantDef,
    pub s: GameState,
    pub rng: SplitMix64,
}

/// A game dealt and sitting at the first lead decision (`startGame` in
/// helpers.ts).
pub fn start_game(players: u8, seed: u64, setup_index: u64) -> Fixture {
    let v = make_variant(players, setup_index, SetupMode::Deck);
    let mut rng = SplitMix64::new(seed);
    let mut s = new_game(&v, &mut rng, setup_index, SetupMode::Deck);
    resolve_chance_mut(&mut s, &v, &mut rng).expect("deal");
    if s.phase == Phase::Mulligan {
        apply_action_mut(&mut s, &v, Action::Mulligan { take: false }).unwrap();
    }
    Fixture { v, s, rng }
}

pub fn actions(f: &Fixture) -> Vec<Action> {
    match get_pending(&f.s, &f.v) {
        Pending::Decision { .. } => {}
        other => panic!("expected decision, got {other:?}"),
    }
    let mut out = Vec::new();
    legal_actions(&f.s, &f.v, &mut out);
    out
}

pub fn actor(f: &Fixture) -> Player {
    match get_pending(&f.s, &f.v) {
        Pending::Decision { player } => player,
        other => panic!("expected decision, got {other:?}"),
    }
}

pub fn apply(f: &mut Fixture, a: Action) {
    apply_action_mut(&mut f.s, &f.v, a).unwrap_or_else(|e| panic!("apply {a:?}: {e}"));
}

/// Resolve any chance nodes standing in the way (`settle` in helpers.ts).
pub fn settle(f: &mut Fixture) {
    for _ in 0..64 {
        if get_pending(&f.s, &f.v) != Pending::Chance {
            return;
        }
        resolve_chance_mut(&mut f.s, &f.v, &mut f.rng).unwrap();
    }
}

/// Replace a hand outright. Cards taken away go to the action discard, so
/// the deck stays conserved and later chapters still deal full hands
/// (`setHand` in helpers.ts).
pub fn set_hand(f: &mut Fixture, player: Player, cards: &[ActionCardId]) {
    let hand = f.s.player_states[player.as_index()].hand;
    for &card in hand.iter() {
        if !cards.contains(&card) {
            f.s.action_discard.push(card);
        }
    }
    f.s.player_states[player.as_index()].hand = cards.iter().copied().collect();
}

/// Card id from suit index (`cardId` in helpers.ts).
pub fn card_id(suit_index: u8, number: u8) -> ActionCardId {
    ActionCardId(suit_index * 7 + (number - 1))
}

pub const ADMIN: u8 = 0;
pub const AGGRESSION: u8 = 1;
pub const CONSTRUCTION: u8 = 2;
pub const MOBILIZATION: u8 = 3;

/// End every remaining turn segment: the TS `while (f.s.turn) endTurn`.
pub fn end_all_turns(f: &mut Fixture) {
    while f.s.turn.is_some() {
        apply(f, Action::EndTurn);
    }
}

/// Follow modes offered for one card, as sorted single letters.
pub fn modes_for(list: &[Action], card: ActionCardId) -> Vec<char> {
    let mut out: Vec<char> = list
        .iter()
        .filter_map(|a| match a {
            Action::Follow { card: c, mode } if *c == card => Some(match mode {
                arcs_engine::FollowMode::Surpass => 's',
                arcs_engine::FollowMode::Copy => 'c',
                arcs_engine::FollowMode::Pivot => 'p',
            }),
            _ => None,
        })
        .collect();
    out.sort_unstable();
    out
}
