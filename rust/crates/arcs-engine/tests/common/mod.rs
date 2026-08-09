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

/// First action matching the predicate (`find` in helpers.ts).
pub fn find(list: &[Action], pred: impl Fn(&Action) -> bool) -> Action {
    *list
        .iter()
        .find(|a| pred(a))
        .expect("no action matching the predicate")
}

/// Put the current actor on turn with one named card, Prelude skipped
/// (`turnWith` in rules.test.ts).
pub fn turn_with(f: &mut Fixture, suit: u8, number: u8) -> Player {
    let player = actor(f);
    set_hand(f, player, &[card_id(suit, number)]);
    apply(
        f,
        Action::Lead {
            card: card_id(suit, number),
        },
    );
    apply(f, Action::BeginActions);
    player
}

// ---------------------------------------------------------------------------
// Powers-test fixtures (`tests/powers.test.ts` helpers)
// ---------------------------------------------------------------------------

use arcs_engine::CourtCardId;
use arcs_engine::court::COURT_DECK;
use arcs_engine::rng::Rng;
use arcs_engine::state::BattleState;
use arcs_engine::types::SystemId;

/// The card id of a Court card by name (`card` in powers.test.ts).
pub fn court(name: &str) -> CourtCardId {
    COURT_DECK
        .iter()
        .find(|c| c.name == name)
        .unwrap_or_else(|| panic!("no card {name}"))
        .id
}

/// Put the actor on turn holding `cards`, having led a card of `suit`
/// (`turnHolding` in powers.test.ts).
pub fn turn_holding(seed: u64, cards: &[&str], suit: u8, number: u8) -> (Fixture, Player) {
    let mut f = start_game(3, seed, 0);
    let player = actor(&f);
    f.s.player_states[player.as_index()].guild_cards = cards.iter().map(|n| court(n)).collect();
    set_hand(&mut f, player, &[card_id(suit, number)]);
    apply(
        &mut f,
        Action::Lead {
            card: card_id(suit, number),
        },
    );
    (f, player)
}

/// The same, but with Battle pips and a planet where the player outnumbers a
/// Rival, ready for a `battle` action (`battleWith` in powers.test.ts).
pub fn battle_with(seed: u64, cards: &[&str]) -> (Fixture, Player, Player, SystemId) {
    let (mut f, player) = turn_holding(seed, cards, AGGRESSION, 2);
    let rival = Player((player.0 + 1) % 3);
    let system = (0..f.s.systems.len())
        .find(|&i| {
            f.v.systems[i].kind == arcs_engine::map::SystemKind::Planet
                && !f.s.systems[i].out_of_play
        })
        .expect("an in-play planet exists");
    let system = SystemId(system as u8);
    f.s.systems[system.as_index()].fresh[player.as_index()] = 6;
    f.s.systems[system.as_index()].fresh[rival.as_index()] = 4;
    apply(&mut f, Action::BeginActions);
    (f, player, rival, system)
}

/// A battle mid-resolution with the roll already made; only the named
/// fields matter (`battleState` in helpers.ts).
pub fn battle_state(system: SystemId, attacker: Player, defender: Player, keys: u8) -> BattleState {
    BattleState {
        system,
        attacker,
        defender,
        keys,
        ..BattleState::default()
    }
}

/// An RNG that always returns the same word — `() => 0.9`-style scripted
/// rolls (`gen_range(6)` gives 5 for `u64::MAX`, 0 for `0`).
pub struct ConstRng(pub u64);

impl Rng for ConstRng {
    fn next_u64(&mut self) -> u64 {
        self.0
    }
}

/// Drive the game forward with a trivial policy until the round advances
/// past the one in progress (`settleRound` in helpers.ts).
pub fn settle_round(f: &mut Fixture) {
    let start_rounds = f.s.stats.rounds;
    for _ in 0..200 {
        if f.s.stats.rounds > start_rounds {
            return;
        }
        match get_pending(&f.s, &f.v) {
            Pending::Over => return,
            Pending::Chance => {
                resolve_chance_mut(&mut f.s, &f.v, &mut f.rng).unwrap();
            }
            Pending::Decision { .. } => {
                let mut acts = Vec::new();
                legal_actions(&f.s, &f.v, &mut acts);
                let prefer = acts
                    .iter()
                    .find(|a| matches!(a, Action::EndTurn))
                    .or_else(|| acts.iter().find(|a| matches!(a, Action::PassInitiative)))
                    .copied()
                    .unwrap_or(acts[0]);
                apply_action_mut(&mut f.s, &f.v, prefer).unwrap();
            }
        }
    }
    panic!("round did not end within the step limit");
}

/// The same, but run to the end of the chapter so scoring happens
/// (`settleToChapterEnd` in helpers.ts).
pub fn settle_to_chapter_end(f: &mut Fixture) {
    let start_chapters = f.s.stats.chapters;
    for _ in 0..400 {
        if f.s.stats.chapters > start_chapters {
            return;
        }
        match get_pending(&f.s, &f.v) {
            Pending::Over => return,
            Pending::Chance => {
                resolve_chance_mut(&mut f.s, &f.v, &mut f.rng).unwrap();
            }
            Pending::Decision { .. } => {
                let mut acts = Vec::new();
                legal_actions(&f.s, &f.v, &mut acts);
                let prefer = acts
                    .iter()
                    .find(|a| matches!(a, Action::PassInitiative))
                    .or_else(|| acts.iter().find(|a| matches!(a, Action::EndTurn)))
                    .copied()
                    .unwrap_or(acts[0]);
                apply_action_mut(&mut f.s, &f.v, prefer).unwrap();
            }
        }
    }
    panic!("chapter did not end within the step limit");
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
