//! The Arcs state machine, ported from `src/engine/game.ts` (which remains
//! the source of truth), plus `new_game` from `src/engine/setup.ts`.
//!
//! The game advances through nodes reported by [`get_pending`]:
//!
//! - [`Pending::Chance`]: call [`resolve_chance_mut`];
//! - [`Pending::Decision`]: pick one of [`legal_actions`], apply it with
//!   [`apply_action_mut`];
//! - [`Pending::Over`]: read final Power with [`standings`].
//!
//! Unlike the TS `getPending`, [`get_pending`] does **not** enumerate the
//! legal actions — pending-kind checks are free, enumeration is on demand.
//!
//! Two engine rulings carried over from the TS header: Ransacking picks the
//! Court card holding the most defender agents (ties leftmost), and free
//! Prelude grants are consumed before card pips, most-restrictive first.
//!
//! A Vox card secured mid-turn or mid-Ransack overlays `pending_vox` on the
//! phase rather than replacing it, so the interrupted flow resumes
//! afterwards. Both [`after_action`] and [`settle_battle`] stall while one
//! is pending.

use crate::action::{Action, FollowMode, HitTarget};
use crate::ambitions::{flip_lowest_unflipped, highest_available, score_chapter};
use crate::cards::{CardAmbition, action_card, suit_actions};
use crate::court::{CourtCardKind, court_card};
use crate::dice::{RollTotals, SKIRMISH_FACES, reroll_skirmish, roll_battle};
use crate::inline_vec::InlineVec;
use crate::map::{SYSTEM_COUNT, SystemKind, is_gate, planet_id};
use crate::player_board::raid_cost;
use crate::powers::{
    apply_card_action, apply_card_prelude, apply_peek_swap, apply_vox, apply_vox_immediate,
    can_declare_on_follow, can_peek, can_reroll_skirmish, card_action_cost, card_actions,
    cartel_squeeze, discard_guild_card, extra_battle_dice, loyal_types, peek_swap_actions,
    peek_target_actions, prelude_card_actions, provoke_outrage, skips_zero_marker,
    steal_guild_card, theft_immune, theft_immunity_card, vox_actions, vox_needs_decision,
    weapon_icons,
};
use crate::rng::Rng;
use crate::setup::{SetupMode, VariantDef, draw_setup};
use crate::state::{
    ActionKindSet, BattleState, Building, CourtSlot, GameState, GameStats, MoveState, Peek,
    PendingVox, PlayedCard, PlayerState, RoundState, SystemState, TrophyKind, TurnState, VoxResume,
};
use crate::types::{
    ActionCardId, ActionKind, AmbitionId, BuildingKind, DieType, Phase, PlayMode, Player,
    ResourceType, SystemId,
};

// ---------------------------------------------------------------------------
// Node inspection
// ---------------------------------------------------------------------------

/// What the game needs next. `Decision` names the player to move; call
/// [`legal_actions`] to enumerate their options.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Pending {
    Over,
    Chance,
    Decision { player: Player },
}

/// Why an action could not be applied. Replaces the TS `throw`; the search
/// contract (mcts2 catches apply errors when computing priors) survives as
/// `Result`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RuleError {
    /// The action does not belong to the current phase.
    WrongPhase,
    /// The action names something the state does not contain.
    Illegal(&'static str),
}

impl core::fmt::Display for RuleError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            RuleError::WrongPhase => write!(f, "action does not belong to this phase"),
            RuleError::Illegal(why) => write!(f, "illegal action: {why}"),
        }
    }
}

impl core::error::Error for RuleError {}

/// The cheap half of the TS `getPending`: kind and player, no enumeration.
pub fn get_pending(s: &GameState, _v: &VariantDef) -> Pending {
    // A pending Vox effect belongs to whoever secured the card, and outranks
    // the phase it interrupted.
    if let Some(pending) = s.pending_vox {
        return Pending::Decision {
            player: pending.player,
        };
    }
    match s.phase {
        Phase::Over => Pending::Over,
        Phase::Deal | Phase::BattleRoll => Pending::Chance,
        Phase::Mulligan => Pending::Decision {
            player: mulligan_player(s),
        },
        Phase::Reinforce => Pending::Decision {
            player: s
                .reinforcing
                .expect("reinforce phase without a reinforcing player"),
        },
        Phase::Play => Pending::Decision {
            player: current_actor(s),
        },
        Phase::PeekTarget | Phase::PeekSwap => Pending::Decision {
            player: s.peek.expect("peek phase without a peek").player,
        },
        Phase::LeaderDraft => unreachable!("LeaderDraft is reserved for Leaders & Lore"),
        _ => Pending::Decision {
            player: s.turn.expect("turn phase without a turn").player,
        },
    }
}

fn current_actor(s: &GameState) -> Player {
    s.round.turn_order.as_slice()[s.round.turn_index as usize]
}

/// 2-player mulligan belongs to the player without initiative (p19).
fn mulligan_player(s: &GameState) -> Player {
    Player((s.initiative.0 + 1) % s.players)
}

// ---------------------------------------------------------------------------
// Legal actions
// ---------------------------------------------------------------------------

/// Enumerate every legal action into `out` (cleared first).
///
/// # The enumeration order is a stable contract
///
/// An agent answers a decision with an **index into this list**
/// (`arcs_agents::Agent::choose`), so the meaning of "index 3" is defined by
/// nothing but the order these actions are pushed. Three things read that
/// index and would be silently corrupted by a reordering refactor:
///
/// - **recorded trajectories** — self-play data stores a chosen index per
///   decision, and reordering would relabel every sample in the archive
///   against a policy that was trained on the old order;
/// - **cross-language policies** — the wasm and PyO3 bindings hand an external
///   net `(features, n_legal)` and take an integer back, with no shared action
///   vocabulary to check it against;
/// - **cached decisions** — the search agents key nodes by `Action`, but the
///   harness and the CLI address moves by position.
///
/// `tests/enumeration_order.rs` pins the exact sequence for a set of fixed
/// seeded states, so a change here fails loudly instead of quietly changing
/// what index 3 means. Adding an action kind at the *end* of a phase's block
/// is the cheap way to extend the vocabulary; inserting one in the middle is a
/// breaking change to every consumer above.
///
/// **Anything persisted must use [`crate::encode_action`] or
/// `arcs_agents::HeadTargets`, never the raw index.** An index is a transport
/// detail valid only for the state that produced it; a canonical key or a set
/// of head targets survives an engine that enumerates differently tomorrow.
pub fn legal_actions(s: &GameState, v: &VariantDef, out: &mut Vec<Action>) {
    out.clear();
    if s.pending_vox.is_some() {
        vox_actions(s, out);
        return;
    }
    match s.phase {
        Phase::Mulligan => {
            out.push(Action::Mulligan { take: false });
            out.push(Action::Mulligan { take: true });
        }
        Phase::Play => play_actions(s, out),
        Phase::Prelude => prelude_actions(s, v, out),
        Phase::Actions => turn_actions(s, v, out),
        Phase::Catapult => catapult_actions(s, v, out),
        Phase::BattleAssign => battle_actions(s, out),
        Phase::BattleReroll => reroll_actions(s, out),
        Phase::Reinforce => {
            for def in &v.systems {
                if def.kind == SystemKind::Gate && !s.systems[def.id.as_index()].out_of_play {
                    out.push(Action::Reinforce { system: def.id });
                }
            }
        }
        Phase::PeekTarget => {
            let peek = s.peek.expect("peekTarget without a peek");
            peek_target_actions(s, peek.player, out);
        }
        Phase::PeekSwap => peek_swap_actions(s, out),
        _ => {}
    }
}

/// Skirmishers: "After you roll in battle, you may reroll a number of
/// skirmish dice up to your total Weapon icons from resources and cards."
///
/// Engine ruling: only *blank* skirmish dice are offered. A skirmish face is
/// either one hit or nothing and the die never hurts the attacker, so
/// rerolling a face that already hit is strictly dominated — it can only
/// lose a hit. Every outcome reachable by rerolling a hit is reachable by
/// rerolling fewer dice. (`rerollActions` in game.ts.)
fn reroll_actions(s: &GameState, out: &mut Vec<Action>) {
    let b = s.battle.expect("battleReroll without a battle");
    let most = weapon_icons(s.player(b.attacker)).min(b.skirmish_blanks);
    for count in 0..=most {
        out.push(Action::RerollSkirmish { count });
    }
}

/// Step 1 / Step 2: lead, or surpass / copy / pivot (p8, p10).
/// (`playActions` in game.ts.)
fn play_actions(s: &GameState, out: &mut Vec<Action>) {
    let player = current_actor(s);
    let hand = &s.player(player).hand;

    if s.round.turn_index == 0 {
        for &card in hand.iter() {
            out.push(Action::Lead { card });
        }
        out.push(Action::PassInitiative);
        return;
    }

    let lead = s.round.lead.expect("follower without a lead");
    let lead_suit = action_card(lead.card).suit;
    for &card in hand.iter() {
        let def = action_card(card);
        if def.suit == lead_suit && def.number > s.round.lead_number {
            out.push(Action::Follow {
                card,
                mode: FollowMode::Surpass,
            });
        }
        if def.suit != lead_suit {
            out.push(Action::Follow {
                card,
                mode: FollowMode::Pivot,
            });
        }
        out.push(Action::Follow {
            card,
            mode: FollowMode::Copy,
        });
    }
}

/// Declare / seize / spend resources, then begin actions (p9, p10, p17,
/// p20). (`preludeActions` in game.ts.)
fn prelude_actions(s: &GameState, v: &VariantDef, out: &mut Vec<Action>) {
    let turn = s.turn.expect("prelude without a turn");
    let p = s.player(turn.player);

    // Declaring and seizing are decided before any Prelude action (p20).
    if !turn.prelude_over && turn.prelude_spent.iter().all(|&n| n == 0) {
        for &ambition in declarable_ambitions(s).iter() {
            out.push(Action::DeclareAmbition { ambition });
        }
        if can_seize(s, turn.player) {
            for &card in p.hand.iter() {
                out.push(Action::Seize { card });
            }
        }
    }

    // Spend resources for their Prelude actions (p17). Outraged types
    // cannot; one offer per distinct held type.
    let loyal = loyal_types(s, turn.player);
    let mut seen = [false; ResourceType::COUNT];
    for slot in 0..p.open_resource_slots() {
        let Some(r) = p.resources[slot] else { continue };
        if seen[r.as_index()] {
            continue;
        }
        seen[r.as_index()] = true;
        let outraged = p.outrage[r.as_index()];
        let psionic_without_lead = r == ResourceType::Psionic && s.round.lead.is_none();
        if !outraged && !psionic_without_lead {
            out.push(Action::SpendResource { slot: slot as u8 });
        }
        // A Loyal card lets any resource be spent as its type, ignoring
        // Outrage on the type being spent as (p20 rules hierarchy,
        // "ignore").
        for &spend_as in loyal.iter() {
            if spend_as == r {
                continue;
            }
            if spend_as == ResourceType::Psionic && s.round.lead.is_none() {
                continue;
            }
            out.push(Action::SpendResourceAs {
                slot: slot as u8,
                spend_as,
            });
        }
    }

    prelude_card_actions(s, v, turn.player, out);
    out.push(Action::BeginActions);
}

/// Which ambitions the current turn may declare.
///
/// The normal route is the leader declaring the ambition printed on a
/// face-up lead card (p9). **Galactic Bards** adds a second: "When you
/// Surpass or Pivot, before taking any actions, you may declare the ambition
/// on your played card if an ambition has not been declared yet this round."
/// (`declarableAmbitions` in game.ts.)
fn declarable_ambitions(s: &GameState) -> InlineVec<AmbitionId, { AmbitionId::COUNT }> {
    let turn = s.turn.expect("prelude without a turn");
    let mut out = InlineVec::filled(AmbitionId::Tycoon);
    if turn.declared_this_turn || s.available_markers.is_empty() {
        return out;
    }

    let card = match turn.mode {
        PlayMode::Lead => match s.round.lead {
            Some(lead) if !lead.face_down => lead.card,
            _ => return out,
        },
        PlayMode::Surpass | PlayMode::Pivot => {
            if !can_declare_on_follow(s, turn.player) || s.round.ambition_declared {
                return out;
            }
            turn.card
        }
        // A Copy play is face down and declares nothing.
        PlayMode::Copy => return out,
    };

    match action_card(card).ambition() {
        CardAmbition::Any => {
            for a in AmbitionId::ALL {
                out.push(a);
            }
        }
        CardAmbition::Some(a) => out.push(a),
        CardAmbition::None => {}
    }
    out
}

/// "You cannot do this if you have the initiative marker or if someone has
/// already seized the initiative this round" (p23, glossary).
fn can_seize(s: &GameState, player: Player) -> bool {
    !s.initiative_seized && player != s.initiative
}

/// Which action kinds the current turn can still pay for.
/// (`availableKinds` in game.ts.)
fn available_kinds(turn: &TurnState) -> ActionKindSet {
    let mut kinds = ActionKindSet::EMPTY;
    if turn.pips_left > 0 {
        kinds.0 |= turn.pip_actions.0;
        if turn.weapon_spent {
            kinds.insert(ActionKind::Battle);
        }
    }
    for grant in turn.free_actions.iter() {
        kinds.0 |= grant.0;
    }
    kinds
}

/// The actions phase (`turnActions` in game.ts).
fn turn_actions(s: &GameState, v: &VariantDef, out: &mut Vec<Action>) {
    let turn = s.turn.expect("actions without a turn");
    let player = turn.player;
    let kinds = available_kinds(&turn);

    if kinds.contains(ActionKind::Tax) {
        tax_actions(s, v, player, out);
    }
    if kinds.contains(ActionKind::Build) {
        build_actions(s, v, player, out);
    }
    if kinds.contains(ActionKind::Move) {
        move_actions(s, v, player, out);
    }
    if kinds.contains(ActionKind::Repair) {
        repair_actions(s, player, out);
    }
    if kinds.contains(ActionKind::Influence) {
        influence_actions(s, player, out);
    }
    if kinds.contains(ActionKind::Secure) {
        secure_actions(s, player, out);
    }
    if kinds.contains(ActionKind::Battle) {
        battle_setup_actions(s, v, player, out);
    }
    card_actions(s, v, player, kinds, out);

    out.push(Action::EndTurn);
}

/// Tax: a Loyal city, or a Rival city in a system you control (p12).
/// (`taxActions` in game.ts.)
fn tax_actions(s: &GameState, v: &VariantDef, player: Player, out: &mut Vec<Action>) {
    for def in &v.systems {
        let st = &s.systems[def.id.as_index()];
        if st.out_of_play {
            continue;
        }
        let controller = s.control_of(def.id);
        for (i, b) in st.buildings.iter().enumerate() {
            if b.kind() != BuildingKind::City || b.taxed_this_turn() {
                continue;
            }
            if b.player() != player && controller != Some(player) {
                continue;
            }
            out.push(Action::Tax {
                system: def.id,
                building: i as u8,
            });
        }
    }
}

/// Build a starport/city in an empty slot, or a ship at a Loyal starport
/// (p12). (`buildActions` in game.ts.)
fn build_actions(s: &GameState, v: &VariantDef, player: Player, out: &mut Vec<Action>) {
    let p = s.player(player);
    for def in &v.systems {
        let st = &s.systems[def.id.as_index()];
        if st.out_of_play {
            continue;
        }

        if def.kind == SystemKind::Planet
            && s.has_piece(def.id, player)
            && st.buildings.len() < def.building_slots as usize
        {
            if p.starports_supply > 0 {
                out.push(Action::BuildBuilding {
                    system: def.id,
                    kind: BuildingKind::Starport,
                });
            }
            if p.cities_used < crate::player_board::CITY_SLOTS as u8 {
                out.push(Action::BuildBuilding {
                    system: def.id,
                    kind: BuildingKind::City,
                });
            }
        }

        if p.ships_supply > 0 {
            for (i, b) in st.buildings.iter().enumerate() {
                if b.kind() == BuildingKind::Starport
                    && b.player() == player
                    && !b.built_this_turn()
                {
                    out.push(Action::BuildShip {
                        system: def.id,
                        building: i as u8,
                    });
                }
            }
        }
    }
}

/// Move any number of Loyal ships to an adjacent system (p13).
/// (`moveActions` in game.ts.)
fn move_actions(s: &GameState, v: &VariantDef, player: Player, out: &mut Vec<Action>) {
    for from in 0..SYSTEM_COUNT {
        let from = SystemId(from as u8);
        if s.systems[from.as_index()].out_of_play {
            continue;
        }
        let count = s.ships_of(from, player);
        if count == 0 {
            continue;
        }
        for &to in s.neighbours(v, from).iter() {
            for n in 1..=count {
                out.push(Action::Move { from, to, ships: n });
            }
        }
    }
}

/// Repair 1 damaged Loyal ship or building, anywhere on the map (p13).
/// (`repairActions` in game.ts.)
fn repair_actions(s: &GameState, player: Player, out: &mut Vec<Action>) {
    for i in 0..SYSTEM_COUNT {
        let st = &s.systems[i];
        if st.out_of_play {
            continue;
        }
        if st.damaged[player.as_index()] > 0 {
            out.push(Action::Repair {
                system: SystemId(i as u8),
                building: None,
            });
        }
        for (bi, b) in st.buildings.iter().enumerate() {
            if b.player() == player && b.damaged() {
                out.push(Action::Repair {
                    system: SystemId(i as u8),
                    building: Some(bi as u8),
                });
            }
        }
    }
}

/// Place 1 agent on any card in the Court (p13).
/// (`influenceActions` in game.ts.)
fn influence_actions(s: &GameState, player: Player, out: &mut Vec<Action>) {
    if s.player(player).agents_supply == 0 {
        return;
    }
    for slot in 0..s.court.len() {
        out.push(Action::Influence { slot: slot as u8 });
    }
}

/// Take a Court card you have strictly the most agents on (p13).
/// (`secureActions` in game.ts.)
fn secure_actions(s: &GameState, player: Player, out: &mut Vec<Action>) {
    'slots: for (i, slot) in s.court.iter().enumerate() {
        let mine = slot.agents[player.as_index()];
        if mine == 0 {
            continue;
        }
        for p in 0..s.players as usize {
            if p != player.as_index() && slot.agents[p] >= mine {
                continue 'slots;
            }
        }
        out.push(Action::Secure { slot: i as u8 });
    }
}

/// Battle: pick a system, a defender and the dice pool (p14). The dice split
/// is part of the action so the whole battle decision is one enumerable
/// choice. (`battleSetupActions` in game.ts.)
fn battle_setup_actions(s: &GameState, _v: &VariantDef, player: Player, out: &mut Vec<Action>) {
    use crate::dice::DICE_PER_TYPE;
    for system in 0..SYSTEM_COUNT {
        let system = SystemId(system as u8);
        if s.systems[system.as_index()].out_of_play {
            continue;
        }
        let attacking = s.ships_of(system, player);
        if attacking == 0 {
            continue;
        }
        for defender in 0..s.players {
            let defender = Player(defender);
            if defender == player || !s.has_piece(system, defender) {
                continue;
            }
            // "Raid dice only if there are defending buildings, or the
            // defender has no Loyal buildings in any systems on the map."
            // (p14)
            let defending_buildings = s.systems[system.as_index()]
                .buildings
                .iter()
                .any(|b| b.player() == defender);
            let raid_allowed = defending_buildings || !has_any_building(s, defender);
            // "1 die per attacking ship ... you cannot collect more than 6
            // of a type". Gatekeepers adds extra dice at gates.
            let max = (attacking as usize + extra_battle_dice(s, player, system) as usize)
                .min(DICE_PER_TYPE * 3);
            for a in 0..=DICE_PER_TYPE.min(max) {
                for sk in 0..=DICE_PER_TYPE.min(max - a) {
                    let max_raid = if raid_allowed {
                        DICE_PER_TYPE.min(max - a - sk)
                    } else {
                        0
                    };
                    for r in 0..=max_raid {
                        if a + sk + r == 0 {
                            continue;
                        }
                        out.push(Action::Battle {
                            system,
                            defender,
                            assault: a as u8,
                            skirmish: sk as u8,
                            raid: r as u8,
                        });
                    }
                }
            }
        }
    }
}

/// Does the defender hold any building anywhere? Gates the raid dice (p14).
/// (`hasAnyBuilding` in board.ts.)
fn has_any_building(s: &GameState, player: Player) -> bool {
    s.systems
        .iter()
        .any(|st| !st.out_of_play && st.buildings.iter().any(|b| b.player() == player))
}

/// Continue or stop a catapult move (p13). (`catapultActions` in game.ts.)
fn catapult_actions(s: &GameState, v: &VariantDef, out: &mut Vec<Action>) {
    let moving = s.moving.expect("catapult without a move");
    out.push(Action::CatapultStop);
    for &to in s.neighbours(v, moving.at).iter() {
        if moving.visited.contains(&to) {
            continue;
        }
        for n in 1..=moving.ships {
            out.push(Action::Catapult { to, ships: n });
        }
    }
}

/// Hits are assigned one at a time so the action space stays enumerable.
/// (`battleActions` in game.ts.)
fn battle_actions(s: &GameState, out: &mut Vec<Action>) {
    let b = s.battle.expect("battleAssign without a battle");
    let st = &s.systems[b.system.as_index()];

    if b.self_hits > 0 {
        if st.fresh[b.attacker.as_index()] > 0 {
            out.push(Action::AssignSelf { fresh: true });
        }
        if st.damaged[b.attacker.as_index()] > 0 {
            out.push(Action::AssignSelf { fresh: false });
        }
        if !out.is_empty() {
            return;
        }
    }

    if b.hits > 0 {
        if st.fresh[b.defender.as_index()] > 0 {
            out.push(Action::AssignHit {
                target: HitTarget::Ship { fresh: true },
            });
        }
        if st.damaged[b.defender.as_index()] > 0 {
            out.push(Action::AssignHit {
                target: HitTarget::Ship { fresh: false },
            });
        }
        if out.is_empty() {
            for (i, bl) in st.buildings.iter().enumerate() {
                if bl.player() == b.defender {
                    out.push(Action::AssignHit {
                        target: HitTarget::Building { building: i as u8 },
                    });
                }
            }
        }
        if !out.is_empty() {
            return;
        }
    }

    if b.building_hits > 0 {
        for (i, bl) in st.buildings.iter().enumerate() {
            if bl.player() == b.defender {
                out.push(Action::AssignHit {
                    target: HitTarget::Building { building: i as u8 },
                });
            }
        }
        if !out.is_empty() {
            return;
        }
    }

    // Raiding needs surviving attacking ships (p14 step 4.5).
    if b.keys > 0 && s.ships_of(b.system, b.attacker) > 0 {
        let victim = s.player(b.defender);
        // Sworn Guardians: "Rivals cannot steal your resources and other
        // Guild cards. (In battle they can steal this first and then spend
        // keys.)"
        let shield = if theft_immune(s, b.defender) {
            theft_immunity_card(s, b.defender)
        } else {
            None
        };
        if let Some(shield) = shield {
            if court_card(shield).raid_cost <= b.keys {
                out.push(Action::RaidCard { card: shield });
            }
        } else {
            for slot in 0..victim.open_resource_slots() {
                if victim.resources[slot].is_some() && raid_cost(slot) <= b.keys {
                    out.push(Action::RaidResource { slot: slot as u8 });
                }
            }
            for &card in victim.guild_cards.iter() {
                if court_card(card).raid_cost <= b.keys {
                    out.push(Action::RaidCard { card });
                }
            }
        }
        out.push(Action::RaidDone);
        return;
    }

    out.push(Action::RaidDone);
}

// ---------------------------------------------------------------------------
// Chance
// ---------------------------------------------------------------------------

/// Resolve the pending chance node (`resolveChanceMut` in game.ts).
pub fn resolve_chance_mut(
    s: &mut GameState,
    v: &VariantDef,
    rng: &mut impl Rng,
) -> Result<(), RuleError> {
    match s.phase {
        Phase::Deal => {
            deal_chapter(s, v, rng);
            Ok(())
        }
        Phase::BattleRoll => {
            let mut b = s
                .battle
                .ok_or(RuleError::Illegal("no battle in progress"))?;

            // A Skirmishers reroll replaces only the blank skirmish dice it
            // names; the rest of the roll stands, and no new intercept can
            // appear because the skirmish die has no intercept face.
            if b.pending_reroll > 0 {
                let rr = reroll_skirmish(b.pending_reroll, rng);
                // Swap the named blanks for their new faces.
                let mut to_swap = b.pending_reroll;
                let skirmish = DieType::Skirmish.as_index();
                b.rolled[skirmish].retain(|&idx| {
                    if to_swap > 0 && SKIRMISH_FACES[idx as usize].hits == 0 {
                        to_swap -= 1;
                        false
                    } else {
                        true
                    }
                });
                for &f in rr.faces.iter() {
                    b.rolled[skirmish].push(f);
                }
                b.hits += rr.hits;
                b.skirmish_blanks = b.skirmish_blanks - b.pending_reroll + rr.blanks;
                b.pending_reroll = 0;
                b.reroll_done = true;
                s.battle = Some(b);
                s.phase = Phase::BattleAssign;
                settle_battle(s, v);
                return Ok(());
            }

            let roll = roll_battle(b.dice, rng);
            // The faces are what the table shows; a bot applying a synthetic
            // outcome via `apply_battle_roll_mut` has no faces and skips
            // this.
            b.rolled = roll.faces;
            s.battle = Some(b);
            apply_battle_roll_mut(s, v, roll.totals)
        }
        _ => Err(RuleError::WrongPhase),
    }
}

/// The deterministic half of the `battleRoll` chance node: apply a roll's
/// totals and advance the battle. Split from [`resolve_chance_mut`] (which
/// feeds it [`roll_battle`]) so a bot can value a battle as the exact
/// expectation over the dice distribution instead of sampling rolls.
/// (`applyBattleRollMut` in game.ts.)
pub fn apply_battle_roll_mut(
    s: &mut GameState,
    v: &VariantDef,
    totals: RollTotals,
) -> Result<(), RuleError> {
    let mut b = s
        .battle
        .ok_or(RuleError::Illegal("no battle in progress"))?;
    b.self_hits = totals.self_hits;
    b.intercept = totals.intercept;
    b.hits = totals.hits;
    b.building_hits = totals.building_hits;
    b.keys = totals.keys;
    b.skirmish_blanks = totals.skirmish_blanks;
    // Step 4.2: the intercept converts into self-hits, once per battle
    // (p14).
    if b.intercept > 0 {
        b.self_hits += s.systems[b.system.as_index()].fresh[b.defender.as_index()];
        b.intercept_resolved = true;
    }
    s.battle = Some(b);
    // Skirmishers: when the attacker holds the card and has Weapon icons,
    // unrerolled blanks open the battleReroll phase.
    if !b.reroll_done
        && b.skirmish_blanks > 0
        && can_reroll_skirmish(s, b.attacker)
        && weapon_icons(s.player(b.attacker)) > 0
    {
        s.phase = Phase::BattleReroll;
        return Ok(());
    }
    s.phase = Phase::BattleAssign;
    settle_battle(s, v);
    Ok(())
}

/// Step 4 of ending a chapter: shuffle everything, deal 6 each (p19).
/// (`dealChapter` in game.ts.)
fn deal_chapter(s: &mut GameState, v: &VariantDef, rng: &mut impl Rng) {
    // Gather deck + discard + all hands, then shuffle. (The TS version
    // shuffles twice; one Fisher-Yates of the full pool is the same
    // distribution — statistical parity, per the port plan.)
    let mut deck: InlineVec<ActionCardId, { crate::cards::ACTION_CARD_COUNT }> = InlineVec::new();
    for &c in s.action_deck.iter() {
        deck.push(c);
    }
    for &c in s.action_discard.iter() {
        deck.push(c);
    }
    for p in 0..s.players as usize {
        for &c in s.player_states[p].hand.iter() {
            deck.push(c);
        }
        s.player_states[p].hand.clear();
    }
    rng.shuffle(deck.as_mut_slice());

    let mut next = 0usize;
    for p in 0..v.players as usize {
        let mut hand = InlineVec::new();
        for _ in 0..v.hand_size {
            hand.push(deck.as_slice()[next]);
            next += 1;
        }
        s.player_states[p].hand = hand;
    }
    // The TS quirk, kept: after a deal the undealt remainder sits in the
    // *discard* and the deck is empty.
    s.action_deck.clear();
    s.action_discard = deck.as_slice()[next..].iter().copied().collect();
    s.round.consecutive_passes = 0;
    // A fresh deal makes every card's location unknown again.
    s.revealed.clear();
    s.declines.clear();

    if v.players == 2 {
        s.phase = Phase::Mulligan;
        return;
    }
    begin_round(s, v);
}

// ---------------------------------------------------------------------------
// Transitions
// ---------------------------------------------------------------------------

/// Apply one action, mutating in place (`applyActionMut` in game.ts).
pub fn apply_action_mut(s: &mut GameState, v: &VariantDef, a: Action) -> Result<(), RuleError> {
    // A pending Vox effect is resolved before anything else can happen.
    if s.pending_vox.is_some() {
        return resolve_vox(s, v, a);
    }

    match a {
        Action::Mulligan { take } => {
            if s.phase != Phase::Mulligan {
                return Err(RuleError::WrongPhase);
            }
            if take {
                let player = mulligan_player(s);
                let p = &mut s.player_states[player.as_index()];
                for &card in p.hand.iter() {
                    s.action_discard.push(card);
                }
                p.hand.clear();
                let mut hand = InlineVec::new();
                for _ in 0..v.hand_size {
                    if s.action_discard.is_empty() {
                        break;
                    }
                    hand.push(s.action_discard.remove(0));
                }
                s.player_states[player.as_index()].hand = hand;
            }
            begin_round(s, v);
            Ok(())
        }
        Action::Lead { card } => {
            if s.phase != Phase::Play || s.round.turn_index != 0 {
                return Err(RuleError::WrongPhase);
            }
            play_card(s, card, PlayMode::Lead)
        }
        Action::Follow { card, mode } => {
            if s.phase != Phase::Play || s.round.turn_index == 0 {
                return Err(RuleError::WrongPhase);
            }
            let mode = match mode {
                FollowMode::Surpass => PlayMode::Surpass,
                FollowMode::Copy => PlayMode::Copy,
                FollowMode::Pivot => PlayMode::Pivot,
            };
            play_card(s, card, mode)
        }
        Action::PassInitiative => {
            if s.phase != Phase::Play {
                return Err(RuleError::WrongPhase);
            }
            pass_initiative(s, v);
            Ok(())
        }
        Action::DeclareAmbition { ambition } => {
            if s.phase != Phase::Prelude {
                return Err(RuleError::WrongPhase);
            }
            let turn = s.turn.ok_or(RuleError::Illegal("no turn in progress"))?;
            declare_ambition(s, v, ambition, turn.player, turn.mode)?;
            // Farseers looks at a Rival hand after any declaration.
            offer_peek(s, turn.player, Phase::Prelude);
            Ok(())
        }
        Action::Seize { card } => {
            if s.phase != Phase::Prelude {
                return Err(RuleError::WrongPhase);
            }
            let turn = s.turn.ok_or(RuleError::Illegal("no turn in progress"))?;
            if !can_seize(s, turn.player) {
                return Err(RuleError::Illegal("cannot seize the initiative"));
            }
            let p = s.player_mut(turn.player);
            let at = p
                .hand
                .position(&card)
                .ok_or(RuleError::Illegal("card not in hand"))?;
            p.hand.remove(at);
            s.round.played.push(PlayedCard {
                player: turn.player,
                card,
                mode: turn.mode,
                face_down: true,
            });
            s.round.seized_by = Some(turn.player);
            s.initiative_seized = true;
            s.stats.seizes += 1;
            Ok(())
        }
        Action::SpendResource { slot } => {
            if s.phase != Phase::Prelude {
                return Err(RuleError::WrongPhase);
            }
            spend_resource(s, slot, None)
        }
        Action::SpendResourceAs { slot, spend_as } => {
            // A Loyal card: the token leaves your board, but its Prelude
            // action is the one printed for the type it is spent as. Legal
            // only via Loyal cards (enumerated in R3); mechanics are here.
            if s.phase != Phase::Prelude {
                return Err(RuleError::WrongPhase);
            }
            spend_resource(s, slot, Some(spend_as))
        }
        Action::BeginActions => {
            if s.phase != Phase::Prelude {
                return Err(RuleError::WrongPhase);
            }
            end_prelude(s);
            s.phase = Phase::Actions;
            Ok(())
        }
        Action::EndTurn => {
            if s.phase != Phase::Actions && s.phase != Phase::Prelude {
                return Err(RuleError::WrongPhase);
            }
            end_turn(s, v);
            Ok(())
        }
        Action::Reinforce { system } => {
            if s.phase != Phase::Reinforce {
                return Err(RuleError::WrongPhase);
            }
            let player = s
                .reinforcing
                .ok_or(RuleError::Illegal("nobody is reinforcing"))?;
            let n = s.player(player).ships_supply.min(3);
            s.systems[system.as_index()].fresh[player.as_index()] += n;
            s.player_mut(player).ships_supply -= n;
            s.reinforcing = None;
            finish_turn(s, v);
            Ok(())
        }
        // Guild-card Prelude abilities (applyCardPrelude in powers.ts).
        Action::CardPrelude { .. } => {
            if s.phase != Phase::Prelude {
                return Err(RuleError::WrongPhase);
            }
            apply_card_prelude(s, v, a)
        }
        // The board actions bought with pips and free grants, plus
        // Guild-card new actions paid for the same way.
        Action::CardAction { .. }
        | Action::Tax { .. }
        | Action::BuildShip { .. }
        | Action::BuildBuilding { .. }
        | Action::Move { .. }
        | Action::Repair { .. }
        | Action::Influence { .. }
        | Action::Secure { .. }
        | Action::Battle { .. } => {
            // Enumerated only in the actions phase; TS also reaches them
            // from the Prelude (performAction ends it implicitly).
            if s.phase != Phase::Actions && s.phase != Phase::Prelude {
                return Err(RuleError::WrongPhase);
            }
            perform_action(s, v, a)
        }
        Action::Catapult { to, ships } => {
            if s.phase != Phase::Catapult {
                return Err(RuleError::WrongPhase);
            }
            let moving = s.moving.ok_or(RuleError::Illegal("no move in progress"))?;
            let player = s
                .turn
                .ok_or(RuleError::Illegal("no turn in progress"))?
                .player;
            move_ships_into(s, v, moving.at, to, player, ships);
            Ok(())
        }
        Action::CatapultStop => {
            if s.phase != Phase::Catapult {
                return Err(RuleError::WrongPhase);
            }
            s.moving = None;
            s.phase = Phase::Actions;
            after_action(s, v);
            Ok(())
        }
        // Battle assignment and raiding run inside a battle, not against
        // pips.
        Action::AssignSelf { .. }
        | Action::AssignHit { .. }
        | Action::RaidResource { .. }
        | Action::RaidCard { .. }
        | Action::RaidDone => {
            if s.phase != Phase::BattleAssign {
                return Err(RuleError::WrongPhase);
            }
            battle_assign(s, v, a)
        }
        Action::RerollSkirmish { count } => {
            if s.phase != Phase::BattleReroll {
                return Err(RuleError::WrongPhase);
            }
            let mut b = s
                .battle
                .ok_or(RuleError::Illegal("no battle in progress"))?;
            if count > b.skirmish_blanks {
                return Err(RuleError::Illegal("more rerolls than blanks"));
            }
            if count > 0 {
                b.pending_reroll = count;
                s.battle = Some(b);
                s.phase = Phase::BattleRoll;
                return Ok(());
            }
            b.reroll_done = true;
            s.battle = Some(b);
            s.phase = Phase::BattleAssign;
            settle_battle(s, v);
            Ok(())
        }
        // Farseers: choose whose hand to look at, then maybe swap.
        Action::PeekTarget { target } => {
            if s.phase != Phase::PeekTarget {
                return Err(RuleError::WrongPhase);
            }
            let mut peek = s.peek.ok_or(RuleError::Illegal("no peek in progress"))?;
            match target {
                None => end_peek(s, v),
                Some(target) => {
                    peek.target = Some(target);
                    s.peek = Some(peek);
                    s.phase = Phase::PeekSwap;
                }
            }
            Ok(())
        }
        Action::PeekSwap { .. } | Action::PeekSwapSkip => {
            if s.phase != Phase::PeekSwap {
                return Err(RuleError::WrongPhase);
            }
            apply_peek_swap(s, a);
            end_peek(s, v);
            Ok(())
        }
        // A `vox` action is only meaningful while a Vox effect is pending,
        // which the top of this function already handles.
        Action::Vox { .. } | Action::VoxSkip => Err(RuleError::Illegal("no Vox effect pending")),
    }
}

/// Farseers: "When you declare an ambition, look at a Rival's hand." Opens
/// the two-step peek, returning to `resume` afterwards. Returns whether it
/// opened. (`offerPeek` in game.ts.)
fn offer_peek(s: &mut GameState, player: Player, resume: Phase) -> bool {
    if !can_peek(s, player) {
        return false;
    }
    let has_target = (0..s.players).any(|p| p != player.0 && !s.player(Player(p)).hand.is_empty());
    if !has_target {
        return false;
    }
    s.peek = Some(Peek {
        player,
        target: None,
        resume,
    });
    s.phase = Phase::PeekTarget;
    true
}

/// Close a Farseers peek and continue whatever it interrupted. The prelude
/// just carries on; a battle or a turn needs its own continuation re-run,
/// since the interrupted call already returned. (`endPeek` in game.ts.)
fn end_peek(s: &mut GameState, v: &VariantDef) {
    let resume = s.peek.expect("endPeek without a peek").resume;
    s.peek = None;
    s.phase = resume;
    if resume == Phase::BattleAssign {
        settle_battle(s, v);
    } else if resume == Phase::Actions {
        after_action(s, v);
    }
}

/// Resolve a pending Vox `When Secured` effect, then resume the flow it
/// interrupted — the rest of the turn's actions, or the rest of the battle.
/// (`resolveVox` in game.ts.)
fn resolve_vox(s: &mut GameState, v: &VariantDef, a: Action) -> Result<(), RuleError> {
    let pending = s
        .pending_vox
        .ok_or(RuleError::Illegal("no Vox effect pending"))?;
    let player = pending.player;
    let resume = match pending.resume {
        VoxResume::Battle => Phase::BattleAssign,
        VoxResume::Actions => Phase::Actions,
    };
    let declare = apply_vox(s, a)?;

    if let Some(ambition) = declare {
        let mode = s.turn.map(|t| t.mode).unwrap_or(PlayMode::Lead);
        declare_ambition(s, v, ambition, player, mode)?;
        // Farseers triggers on any declaration, including this one.
        if offer_peek(s, player, resume) {
            return Ok(());
        }
    }

    s.phase = resume;
    if resume == Phase::BattleAssign {
        settle_battle(s, v);
    } else {
        after_action(s, v);
    }
    Ok(())
}

// --- playing a card --------------------------------------------------------

/// (`playCard` in game.ts.)
fn play_card(s: &mut GameState, card: ActionCardId, mode: PlayMode) -> Result<(), RuleError> {
    let player = current_actor(s);
    let def = action_card(card);

    // Public memory: a follower who does not Surpass is telling the table
    // something about their hand. Record it before the lead can change.
    if (mode == PlayMode::Copy || mode == PlayMode::Pivot)
        && let Some(lead) = s.round.lead
        && !s.declines.is_full()
    {
        s.declines.push(crate::state::Decline {
            player,
            suit: action_card(lead.card).suit,
            number: s.round.lead_number,
        });
    }

    {
        let p = s.player_mut(player);
        let at = p
            .hand
            .position(&card)
            .ok_or(RuleError::Illegal("card not in hand"))?;
        p.hand.remove(at);
    }
    let played = PlayedCard {
        player,
        card,
        mode,
        face_down: mode == PlayMode::Copy,
    };
    s.round.played.push(played);
    s.stats.cards_played += 1;

    let (pips, pip_actions) = match mode {
        PlayMode::Lead => {
            s.round.lead = Some(played);
            s.round.lead_number = def.number;
            s.round.consecutive_passes = 0;
            (def.pips, ActionKindSet::from_kinds(suit_actions(def.suit)))
        }
        PlayMode::Surpass => {
            // "You Surpass with a 7 action card" seizes the initiative (p10).
            if def.number == 7 && can_seize(s, player) {
                s.round.seized_by = Some(player);
                s.initiative_seized = true;
                s.stats.seizes += 1;
            }
            (def.pips, ActionKindSet::from_kinds(suit_actions(def.suit)))
        }
        PlayMode::Pivot => (1, ActionKindSet::from_kinds(suit_actions(def.suit))),
        PlayMode::Copy => {
            let lead = s
                .round
                .lead
                .ok_or(RuleError::Illegal("copy without a lead"))?;
            (
                1,
                ActionKindSet::from_kinds(suit_actions(action_card(lead.card).suit)),
            )
        }
    };

    s.turn = Some(TurnState {
        player,
        mode,
        card,
        pips_left: pips,
        pip_actions,
        free_actions: InlineVec::new(),
        weapon_spent: false,
        prelude_over: false,
        declared_this_turn: false,
        prelude_spent: [0; ResourceType::COUNT],
        secured_this_prelude: InlineVec::new(),
        card_preludes_used: InlineVec::new(),
    });
    s.phase = Phase::Prelude;
    Ok(())
}

/// Declare an ambition (p9). `player` is the declarer, which is the player
/// on turn for the normal route but can be a securing player for Populist
/// Demands (R3). (`declareAmbition` in game.ts.)
fn declare_ambition(
    s: &mut GameState,
    v: &VariantDef,
    ambition: AmbitionId,
    player: Player,
    mode: PlayMode,
) -> Result<(), RuleError> {
    let marker =
        highest_available(s, v).ok_or(RuleError::Illegal("no ambition marker available"))?;
    let at = s
        .available_markers
        .position(&marker)
        .expect("highest_available returned an unavailable marker");
    s.available_markers.remove(at);
    s.declared[ambition.as_index()].push(marker);
    if let Some(turn) = s.turn.as_mut()
        && turn.player == player
    {
        turn.declared_this_turn = true;
    }
    s.round.ambition_declared = true;
    // "Place the zero marker onto the lead card ... its card number is now
    // 0." Secret Order exempts Keeper and Empath; Galactic Bards exempts
    // its own Surpass/Pivot declaration.
    if !skips_zero_marker(s, player, ambition, mode) {
        s.round.lead_number = 0;
    }
    s.stats.ambitions_declared += 1;
    Ok(())
}

/// Prelude resource actions (p17). `spend_as` is set when a Loyal Guild card
/// lets the token be spent as a different type: the token returned to the
/// supply is the real one, but the action taken is the one printed for
/// `spend_as`. (`spendResource` in game.ts.)
fn spend_resource(
    s: &mut GameState,
    slot: u8,
    spend_as: Option<ResourceType>,
) -> Result<(), RuleError> {
    let mut turn = s.turn.ok_or(RuleError::Illegal("no turn in progress"))?;
    let lead = s.round.lead;
    let p = s.player_mut(turn.player);
    let real = p
        .resources
        .get(slot as usize)
        .copied()
        .flatten()
        .ok_or(RuleError::Illegal("no resource in that slot"))?;
    let r = spend_as.unwrap_or(real);
    if r == ResourceType::Psionic && lead.is_none() {
        return Err(RuleError::Illegal("a Psionic needs a lead card to copy"));
    }
    p.resources[slot as usize] = None;
    turn.prelude_spent[real.as_index()] += 1;

    match r {
        ResourceType::Material => turn.free_actions.push(ActionKindSet::from_kinds(&[
            crate::types::ActionKind::Build,
            crate::types::ActionKind::Repair,
        ])),
        ResourceType::Fuel => turn
            .free_actions
            .push(ActionKindSet::from_kinds(&[crate::types::ActionKind::Move])),
        ResourceType::Relic => turn.free_actions.push(ActionKindSet::from_kinds(&[
            crate::types::ActionKind::Secure,
        ])),
        ResourceType::Psionic => {
            let lead = lead.expect("checked above");
            turn.free_actions
                .push(ActionKindSet::from_kinds(suit_actions(
                    action_card(lead.card).suit,
                )));
        }
        // "This turn, you may spend any action pips from your card play to
        // take Battle actions." — it modifies the card's pips, not a free
        // action.
        ResourceType::Weapon => turn.weapon_spent = true,
    }
    s.turn = Some(turn);
    Ok(())
}

/// Spent Prelude resources return to the supply when the Prelude ends (p20).
/// (`endPrelude` in game.ts; the caller sets the phase.)
fn end_prelude(s: &mut GameState) {
    let Some(mut turn) = s.turn else { return };
    if !turn.prelude_over {
        for r in ResourceType::ALL {
            for _ in 0..turn.prelude_spent[r.as_index()] {
                s.return_to_supply(r);
            }
            turn.prelude_spent[r.as_index()] = 0;
        }
        turn.prelude_over = true;
    }
    s.turn = Some(turn);
}

/// Pay for one action. Free grants from resources are consumed before card
/// pips, most-restrictive first (engine ruling — see the game.ts header).
pub fn pay_for(turn: &mut TurnState, kind: crate::types::ActionKind) -> Result<(), RuleError> {
    let mut best: Option<usize> = None;
    for (i, grant) in turn.free_actions.iter().enumerate() {
        if !grant.contains(kind) {
            continue;
        }
        if best.is_none_or(|b| grant.len() < turn.free_actions.as_slice()[b].len()) {
            best = Some(i);
        }
    }
    if let Some(i) = best {
        turn.free_actions.remove(i);
        return Ok(());
    }
    if turn.pips_left > 0
        && (turn.pip_actions.contains(kind)
            || (turn.weapon_spent && kind == crate::types::ActionKind::Battle))
    {
        turn.pips_left -= 1;
        return Ok(());
    }
    Err(RuleError::Illegal("cannot pay for that action"))
}

/// [`pay_for`] against the turn stored in the state.
fn pay(s: &mut GameState, kind: ActionKind) -> Result<(), RuleError> {
    let mut turn = s.turn.ok_or(RuleError::Illegal("no turn in progress"))?;
    pay_for(&mut turn, kind)?;
    s.turn = Some(turn);
    Ok(())
}

// --- standard actions ------------------------------------------------------

/// (`performAction` in game.ts.)
fn perform_action(s: &mut GameState, v: &VariantDef, a: Action) -> Result<(), RuleError> {
    end_prelude(s);
    s.phase = Phase::Actions;
    let player = s
        .turn
        .ok_or(RuleError::Illegal("no turn in progress"))?
        .player;

    match a {
        Action::Tax { system, building } => {
            pay(s, ActionKind::Tax)?;
            let owner = {
                let st = &mut s.systems[system.as_index()];
                let b = st
                    .buildings
                    .as_mut_slice()
                    .get_mut(building as usize)
                    .ok_or(RuleError::Illegal("no such building"))?;
                b.set_taxed_this_turn();
                b.player()
            };
            if let Some(t) = v.systems[system.as_index()].planet_type {
                s.take_from_supply(player, t);
            }
            if owner != player {
                capture_agent(s, player, owner);
            }
        }
        Action::BuildShip { system, building } => {
            pay(s, ActionKind::Build)?;
            {
                let p = s.player_mut(player);
                if p.ships_supply == 0 {
                    return Err(RuleError::Illegal("no ships in supply"));
                }
                p.ships_supply -= 1;
            }
            let controller = s.control_of(system);
            let st = &mut s.systems[system.as_index()];
            st.buildings
                .as_mut_slice()
                .get_mut(building as usize)
                .ok_or(RuleError::Illegal("no such building"))?
                .set_built_this_turn();
            if controller.is_some_and(|c| c != player) {
                st.damaged[player.as_index()] += 1;
            } else {
                st.fresh[player.as_index()] += 1;
            }
        }
        Action::BuildBuilding { system, kind } => {
            pay(s, ActionKind::Build)?;
            if s.systems[system.as_index()].buildings.is_full() {
                return Err(RuleError::Illegal("no free building slot"));
            }
            let controller = s.control_of(system);
            let damaged = controller.is_some_and(|c| c != player);
            {
                let p = s.player_mut(player);
                match kind {
                    BuildingKind::City => {
                        if p.cities_used >= crate::player_board::CITY_SLOTS as u8 {
                            return Err(RuleError::Illegal("no cities left"));
                        }
                        p.cities_used += 1;
                    }
                    BuildingKind::Starport => {
                        if p.starports_supply == 0 {
                            return Err(RuleError::Illegal("no starports in supply"));
                        }
                        p.starports_supply -= 1;
                    }
                }
            }
            s.systems[system.as_index()]
                .buildings
                .push(Building::new(player, kind, damaged));
            if kind == BuildingKind::City {
                s.compact_resources_of(player);
            }
        }
        Action::Move { from, to, ships } => {
            pay(s, ActionKind::Move)?;
            move_ships_into(s, v, from, to, player, ships);
            return Ok(()); // may open a catapult decision
        }
        Action::Repair { system, building } => {
            pay(s, ActionKind::Repair)?;
            let st = &mut s.systems[system.as_index()];
            match building {
                None => {
                    if st.damaged[player.as_index()] == 0 {
                        return Err(RuleError::Illegal("no damaged ship there"));
                    }
                    st.damaged[player.as_index()] -= 1;
                    st.fresh[player.as_index()] += 1;
                }
                Some(bi) => {
                    st.buildings
                        .as_mut_slice()
                        .get_mut(bi as usize)
                        .ok_or(RuleError::Illegal("no such building"))?
                        .set_damaged(false);
                }
            }
        }
        Action::Influence { slot } => {
            pay(s, ActionKind::Influence)?;
            {
                let p = s.player_mut(player);
                if p.agents_supply == 0 {
                    return Err(RuleError::Illegal("no agents in supply"));
                }
                p.agents_supply -= 1;
            }
            s.court
                .as_mut_slice()
                .get_mut(slot as usize)
                .ok_or(RuleError::Illegal("no such court slot"))?
                .agents[player.as_index()] += 1;
        }
        Action::Secure { slot } => {
            pay(s, ActionKind::Secure)?;
            secure_card(s, v, player, slot as usize, false)?;
        }
        Action::CardAction { card, name, .. } => {
            pay(s, card_action_cost(card, name)?)?;
            apply_card_action(s, player, a)?;
        }
        Action::Battle {
            system,
            defender,
            assault,
            skirmish,
            raid,
        } => {
            pay(s, ActionKind::Battle)?;
            s.battle = Some(BattleState {
                rolled: Default::default(),
                system,
                attacker: player,
                defender,
                // Indexed by `DieType::as_index`: assault, skirmish, raid.
                dice: [assault, skirmish, raid],
                self_hits: 0,
                intercept: 0,
                hits: 0,
                building_hits: 0,
                keys: 0,
                intercept_resolved: false,
                skirmish_blanks: 0,
                pending_reroll: 0,
                reroll_done: false,
            });
            s.stats.battles += 1;
            s.phase = Phase::BattleRoll;
            return Ok(());
        }
        _ => return Err(RuleError::Illegal("not a standard action")),
    }
    after_action(s, v);
    Ok(())
}

/// Move ships, then offer Catapult moves when they left a Loyal starport and
/// landed on a gate nobody else controls (p13).
///
/// "When you take a Catapult move, check for control before moving in, not
/// after" (p13) — so the destination's controller is read before the ships
/// arrive, otherwise the arriving ships would clear the blockade themselves.
/// (`moveShipsInto` in game.ts.)
fn move_ships_into(
    s: &mut GameState,
    v: &VariantDef,
    from: SystemId,
    to: SystemId,
    player: Player,
    ships: u8,
) {
    let catapulting = s.phase == Phase::Catapult;
    let mut visited: InlineVec<SystemId, SYSTEM_COUNT> = if catapulting {
        s.moving.expect("catapult without a move").visited
    } else {
        InlineVec::from_slice(&[from])
    };
    let controller_before = s.control_of(to);

    // Move fresh ships first, then damaged ones.
    let pi = player.as_index();
    let mut left = ships;
    let fresh = s.systems[from.as_index()].fresh[pi].min(left);
    s.systems[from.as_index()].fresh[pi] -= fresh;
    s.systems[to.as_index()].fresh[pi] += fresh;
    left -= fresh;
    let damaged = s.systems[from.as_index()].damaged[pi].min(left);
    s.systems[from.as_index()].damaged[pi] -= damaged;
    s.systems[to.as_index()].damaged[pi] += damaged;

    // Loyal starport only — "you can't Catapult from Rival starports you
    // control".
    let can_catapult = catapulting || s.has_building(from, player, Some(BuildingKind::Starport));
    // "keep moving ... until they move to a gate controlled by anyone else
    // or they move to any planet"
    let blocked = !is_gate(to) || controller_before.is_some_and(|c| c != player);

    visited.push(to);
    let onward = s.neighbours(v, to).iter().any(|n| !visited.contains(n));
    if can_catapult && !blocked && ships > 0 && onward {
        s.moving = Some(MoveState {
            at: to,
            ships,
            visited,
        });
        s.phase = Phase::Catapult;
        return;
    }
    s.moving = None;
    s.phase = Phase::Actions;
    after_action(s, v);
}

/// Take a Court card, capture the Rival agents on it, refill the slot (p13).
/// (`secureCard` in game.ts.)
fn secure_card(
    s: &mut GameState,
    _v: &VariantDef,
    player: Player,
    slot: usize,
    ransack: bool,
) -> Result<(), RuleError> {
    let court_slot = *s
        .court
        .as_slice()
        .get(slot)
        .ok_or(RuleError::Illegal("no such court slot"))?;
    let card = court_slot.card;

    s.player_mut(player).agents_supply += court_slot.agents[player.as_index()];
    for p in 0..s.players as usize {
        if p == player.as_index() {
            continue;
        }
        let n = court_slot.agents[p];
        if n == 0 {
            continue;
        }
        if ransack {
            // Ransacking takes agents as Trophies, not Captives (p16).
            // game.ts:1135 records them with kind 'ship', which would send
            // them home to the *ship* supply at Warlord scoring; the port
            // keeps them agents so every piece returns to its own pool.
            s.player_mut(player).trophies[p][TrophyKind::Agent.as_index()] += n;
        } else {
            s.player_mut(player).captives[p] += n;
        }
    }

    if court_card(card).kind == CourtCardKind::Vox {
        // A Vox card resolves When Secured. The ones that need a decision
        // overlay `pending_vox` on the current phase; `after_action` and
        // `settle_battle` stall until it is answered, then resume where
        // they left off.
        if vox_needs_decision(card) {
            s.pending_vox = Some(PendingVox {
                card,
                player,
                resume: if s.phase == Phase::BattleAssign {
                    VoxResume::Battle
                } else {
                    VoxResume::Actions
                },
            });
        } else {
            apply_vox_immediate(s, player, card);
        }
    } else {
        crate::powers::gain_guild_card(s, player, card);
        if let Some(turn) = s.turn.as_mut()
            && !turn.prelude_over
        {
            // A card secured during the Prelude cannot use its own Prelude
            // ability this turn (p20). Entries after the Prelude ended are
            // dead, so they are not recorded (the TS list records them;
            // nothing ever reads them).
            turn.secured_this_prelude.push(card);
        }
    }

    // Securing is a decision, not a chance node — the Court deck was
    // shuffled at setup and is drawn in order. If it ever empties, the
    // discard comes back in reverse, which keeps the refill a deterministic
    // function of the state rather than smuggling in an RNG the caller did
    // not provide.
    if s.court_deck.is_empty() {
        s.court_deck = s.court_discard.iter().rev().copied().collect();
        s.court_discard.clear();
    }
    match s.court_deck.pop() {
        Some(next) => {
            s.court.as_mut_slice()[slot] = CourtSlot {
                card: next,
                agents: [0; crate::state::MAX_SEATS],
            };
        }
        None => {
            s.court.remove(slot);
        }
    }
    Ok(())
}

/// Taxing a Rival city captures one of their agents (p12).
/// (`captureAgent` in game.ts.)
fn capture_agent(s: &mut GameState, captor: Player, victim: Player) {
    if s.player(victim).agents_supply == 0 {
        return;
    }
    s.player_mut(victim).agents_supply -= 1;
    s.player_mut(captor).captives[victim.as_index()] += 1;
}

// --- battle ----------------------------------------------------------------

/// (`battleAssign` in game.ts.)
fn battle_assign(s: &mut GameState, v: &VariantDef, a: Action) -> Result<(), RuleError> {
    let mut b = s
        .battle
        .ok_or(RuleError::Illegal("no battle in progress"))?;

    match a {
        Action::AssignSelf { fresh } => {
            if b.self_hits == 0 {
                return Err(RuleError::Illegal("no self-hits to assign"));
            }
            b.self_hits -= 1;
            s.battle = Some(b);
            hit_ship(s, b.system, b.attacker, fresh, b.defender)?;
        }
        Action::AssignHit {
            target: HitTarget::Ship { fresh },
        } => {
            if b.hits == 0 {
                return Err(RuleError::Illegal("no hits to assign"));
            }
            b.hits -= 1;
            s.battle = Some(b);
            hit_ship(s, b.system, b.defender, fresh, b.attacker)?;
        }
        Action::AssignHit {
            target: HitTarget::Building { building },
        } => {
            // Hits spill onto buildings only once no defending ships
            // remain.
            let st = &s.systems[b.system.as_index()];
            let defender_ships =
                st.fresh[b.defender.as_index()] + st.damaged[b.defender.as_index()];
            if b.hits > 0 && defender_ships == 0 {
                b.hits -= 1;
            } else if b.building_hits > 0 {
                b.building_hits -= 1;
            } else {
                return Err(RuleError::Illegal("no building hits to assign"));
            }
            s.battle = Some(b);
            hit_building(s, v, b.system, building as usize, b.attacker);
        }
        Action::RaidResource { slot } => {
            let r = s
                .player(b.defender)
                .resources
                .get(slot as usize)
                .copied()
                .flatten()
                .ok_or(RuleError::Illegal("no resource in that slot"))?;
            let cost = raid_cost(slot as usize);
            if cost > b.keys {
                return Err(RuleError::Illegal("not enough keys"));
            }
            b.keys -= cost;
            s.battle = Some(b);
            s.player_mut(b.defender).resources[slot as usize] = None;
            // Stolen straight into the attacker's slots (no supply
            // round-trip); discarded when there is no room.
            if !s.player_mut(b.attacker).gain_resource(r) {
                s.return_to_supply(r);
            }
        }
        Action::RaidCard { card } => {
            if !s.player(b.defender).guild_cards.contains(&card) {
                return Err(RuleError::Illegal("defender does not hold that card"));
            }
            let cost = court_card(card).raid_cost;
            if cost > b.keys {
                return Err(RuleError::Illegal("not enough keys"));
            }
            b.keys -= cost;
            s.battle = Some(b);
            steal_guild_card(s, b.defender, b.attacker, card);
        }
        Action::RaidDone => {
            b.keys = 0;
            s.battle = Some(b);
        }
        _ => return Err(RuleError::Illegal("not a battle action")),
    }
    settle_battle(s, v);
    Ok(())
}

/// Damage a fresh ship, or destroy a damaged one into `taker`'s Trophies.
/// (`hitShip` in game.ts.)
fn hit_ship(
    s: &mut GameState,
    system: SystemId,
    owner: Player,
    fresh: bool,
    taker: Player,
) -> Result<(), RuleError> {
    let st = &mut s.systems[system.as_index()];
    let oi = owner.as_index();
    if fresh {
        if st.fresh[oi] == 0 {
            return Err(RuleError::Illegal("no fresh ship to hit"));
        }
        st.fresh[oi] -= 1;
        st.damaged[oi] += 1;
        return Ok(());
    }
    if st.damaged[oi] == 0 {
        return Err(RuleError::Illegal("no damaged ship to hit"));
    }
    st.damaged[oi] -= 1;
    s.player_mut(taker).trophies[oi][TrophyKind::Ship.as_index()] += 1;
    Ok(())
}

/// (`hitBuilding` in game.ts.)
fn hit_building(s: &mut GameState, v: &VariantDef, system: SystemId, index: usize, taker: Player) {
    let st = &mut s.systems[system.as_index()];
    let Some(&b) = st.buildings.as_slice().get(index) else {
        return;
    };
    if !b.damaged() {
        st.buildings.as_mut_slice()[index].set_damaged(true);
        return;
    }
    st.buildings.remove(index);
    let kind = match b.kind() {
        BuildingKind::City => TrophyKind::City,
        BuildingKind::Starport => TrophyKind::Starport,
    };
    s.player_mut(taker).trophies[b.player().as_index()][kind.as_index()] += 1;
    if b.kind() == BuildingKind::City {
        destroy_city(s, v, taker, b.player(), system);
    }
}

/// Provoke Outrage, then Ransack the Court (p16).
/// (`destroyCity` in game.ts.)
fn destroy_city(
    s: &mut GameState,
    v: &VariantDef,
    destroyer: Player,
    victim: Player,
    system: SystemId,
) {
    if let Some(t) = v.systems[system.as_index()].planet_type {
        provoke_outrage(s, destroyer, t);
    }

    // Ransack: secure a card holding any of the victim's agents. Engine
    // ruling — pick the card with the most of them, ties leftmost.
    let mut target = None;
    let mut most = 0u8;
    for (i, slot) in s.court.iter().enumerate() {
        if slot.agents[victim.as_index()] > most {
            most = slot.agents[victim.as_index()];
            target = Some(i);
        }
    }
    if let Some(i) = target {
        secure_card(s, v, destroyer, i, true).expect("ransack target exists");
    }
}

/// Advance or finish the battle once a hit has been assigned.
/// (`settleBattle` in game.ts.)
fn settle_battle(s: &mut GameState, v: &VariantDef) {
    // A Ransack can secure a Vox card mid-battle; answer it before
    // continuing.
    if s.pending_vox.is_some() {
        return;
    }
    let Some(mut b) = s.battle else { return };
    let st = &s.systems[b.system.as_index()];
    let ai = b.attacker.as_index();
    let di = b.defender.as_index();

    // Drop hits that have nothing left to land on.
    if b.self_hits > 0 && st.fresh[ai] + st.damaged[ai] == 0 {
        b.self_hits = 0;
    }
    let defender_ships = st.fresh[di] + st.damaged[di];
    let defender_buildings = st.buildings.iter().any(|x| x.player() == b.defender);
    if b.hits > 0 && defender_ships == 0 && !defender_buildings {
        b.hits = 0;
    }
    if b.building_hits > 0 && !defender_buildings {
        b.building_hits = 0;
    }
    if b.keys > 0 {
        let victim = s.player(b.defender);
        // Sworn Guardians narrows the raidable set to the shield card alone.
        let shield = if theft_immune(s, b.defender) {
            theft_immunity_card(s, b.defender)
        } else {
            None
        };
        let can_steal = match shield {
            Some(c) => court_card(c).raid_cost <= b.keys,
            None => {
                victim
                    .resources
                    .iter()
                    .enumerate()
                    .any(|(i, r)| r.is_some() && raid_cost(i) <= b.keys)
                    || victim
                        .guild_cards
                        .iter()
                        .any(|&c| court_card(c).raid_cost <= b.keys)
            }
        };
        if !can_steal || s.ships_of(b.system, b.attacker) == 0 {
            b.keys = 0;
        }
    }

    if b.self_hits == 0 && b.hits == 0 && b.building_hits == 0 && b.keys == 0 {
        s.battle = None;
        s.phase = Phase::Actions;
        after_action(s, v);
        return;
    }
    s.battle = Some(b);
    s.phase = Phase::BattleAssign;
}

// --- turn / round / chapter flow -------------------------------------------

/// After any action: if nothing can still be spent, the turn ends itself.
/// (`afterAction` in game.ts.)
fn after_action(s: &mut GameState, v: &VariantDef) {
    // A secured Vox card is answered before the turn can advance or end.
    if s.pending_vox.is_some() {
        return;
    }
    let Some(turn) = s.turn else { return };
    if turn.pips_left == 0 && turn.free_actions.is_empty() {
        end_turn(s, v);
    }
}

/// (`endTurn` in game.ts.)
fn end_turn(s: &mut GameState, v: &VariantDef) {
    end_prelude(s);
    let player = s.turn.expect("endTurn without a turn").player;

    // "Rarely, a player will have no starports or ships on the map ... they
    // place 3 fresh ships in any gate at the end of their turn." (p22)
    if s.is_wiped_out(player) && s.player(player).ships_supply > 0 {
        s.reinforcing = Some(player);
        s.phase = Phase::Reinforce;
        return;
    }
    finish_turn(s, v);
}

/// (`finishTurn` in game.ts.)
fn finish_turn(s: &mut GameState, v: &VariantDef) {
    // Per-turn flags reset (tax limit p12, starport build limit p12).
    for st in s.systems.iter_mut() {
        for b in st.buildings.as_mut_slice() {
            b.clear_turn_flags();
        }
    }
    s.turn = None;
    s.battle = None;
    s.moving = None;
    s.round.turn_index += 1;
    s.phase = Phase::Play;
    ensure_actor(s, v);
}

/// (`beginRound` in game.ts.)
fn begin_round(s: &mut GameState, v: &VariantDef) {
    s.round.turn_order = (0..v.players)
        .map(|i| Player((s.initiative.0 + i) % v.players))
        .collect();
    s.round.turn_index = 0;
    s.round.lead = None;
    s.round.lead_number = 0;
    s.round.played.clear();
    s.round.seized_by = None;
    s.round.ambition_declared = false;
    s.initiative_seized = false;
    s.turn = None;
    s.phase = Phase::Play;
    ensure_actor(s, v);
}

/// Skip card-less players; a card-less initiative holder must pass (p8).
/// (`ensureActor` in game.ts.)
fn ensure_actor(s: &mut GameState, v: &VariantDef) {
    loop {
        if s.round.turn_index as usize >= s.round.turn_order.len() {
            end_round(s, v);
            return;
        }
        let player = current_actor(s);
        if !s.player(player).hand.is_empty() {
            return;
        }
        if s.round.turn_index == 0 {
            pass_initiative(s, v);
            return;
        }
        s.round.turn_index += 1;
    }
}

/// "Give the initiative marker to the next clockwise player who has any
/// cards in their hand, then immediately end the round." (p8)
/// (`passInitiative` in game.ts.)
fn pass_initiative(s: &mut GameState, v: &VariantDef) {
    let from = s
        .round
        .turn_order
        .as_slice()
        .get(s.round.turn_index as usize)
        .copied()
        .unwrap_or(s.initiative);
    let with_cards = (0..s.players as usize)
        .filter(|&p| !s.player_states[p].hand.is_empty())
        .count() as u8;
    s.round.consecutive_passes += 1;

    let mut next = None;
    for k in 1..=v.players {
        let cand = Player((from.0 + k) % v.players);
        if !s.player(cand).hand.is_empty() {
            next = Some(cand);
            break;
        }
    }
    let Some(next) = next else {
        end_chapter(s, v);
        return;
    };
    if with_cards == 0 {
        end_chapter(s, v);
        return;
    }
    s.initiative = next;
    discard_played(s);
    s.stats.rounds += 1;

    // "If everyone with cards in their hand passes consecutively ... end the
    // chapter." (p8)
    if s.round.consecutive_passes >= with_cards {
        end_chapter(s, v);
        return;
    }
    begin_round(s, v);
}

/// Discard the round's played cards — but first hand out any card a Union
/// is attached to: "When the round ends, draw that card into your hand and
/// discard this card." (p20) (`discardPlayed` in game.ts.)
fn discard_played(s: &mut GameState) {
    // Action card -> claiming player, first Union on a card wins.
    let unions = s.unions;
    s.unions.clear();
    let mut claimed: InlineVec<(ActionCardId, Player), 4> = InlineVec::new();
    for u in unions.iter() {
        if s.round.played.iter().any(|c| c.card == u.target)
            && !claimed.iter().any(|(t, _)| *t == u.target)
        {
            claimed.push((u.target, u.player));
        }
        discard_guild_card(s, u.player, u.card);
    }

    for i in 0..s.round.played.len() {
        let c = s.round.played.as_slice()[i];
        if let Some(&(_, claimer)) = claimed.iter().find(|(t, _)| *t == c.card) {
            // A Union pulls the card back into a hand. Everyone saw it, but
            // it is no longer out of play, so it does not join `revealed` —
            // the conservative choice loses information rather than
            // inventing an impossible world.
            s.player_mut(claimer).hand.push(c.card);
        } else {
            s.action_discard.push(c.card);
            // Played face up: the whole table watched this card leave play.
            if !c.face_down {
                s.revealed.push(c.card);
            }
        }
    }
    s.round.played.clear();
}

/// Steps 3 and 4 of a round: check initiative, discard, next round (p11).
/// (`endRound` in game.ts.)
fn end_round(s: &mut GameState, v: &VariantDef) {
    if let Some(seized) = s.round.seized_by {
        s.initiative = seized;
    } else {
        let mut best = 0u8;
        let mut winner = None;
        for c in s.round.played.iter() {
            if c.mode != PlayMode::Surpass {
                continue;
            }
            let n = action_card(c.card).number;
            if n > best {
                best = n;
                winner = Some(c.player);
            }
        }
        if let Some(w) = winner {
            s.initiative = w;
        }
    }

    discard_played(s);
    s.stats.rounds += 1;

    if (0..s.players as usize).any(|p| !s.player_states[p].hand.is_empty()) {
        begin_round(s, v);
    } else {
        end_chapter(s, v);
    }
}

/// Ending a chapter: score, clean up, flip, end or advance (p18-p19).
/// (`endChapter` in game.ts.)
fn end_chapter(s: &mut GameState, v: &VariantDef) {
    discard_played(s);
    for p in 0..s.players as usize {
        let hand = s.player_states[p].hand;
        for &card in hand.iter() {
            s.action_discard.push(card);
        }
        s.player_states[p].hand.clear();
    }

    let scores = score_chapter(s, v);
    for p in 0..s.players as usize {
        s.player_states[p].power += scores.gained[p];
    }

    let scored = |a: AmbitionId| scores.results.iter().any(|r| r.ambition == a);
    if scored(AmbitionId::Warlord) {
        return_trophies(s);
    }
    if scored(AmbitionId::Tyrant) {
        return_captives(s);
    }
    // "After scoring, Rivals discard all Material / Fuel" onto a held
    // Cartel card (p20).
    cartel_squeeze(s);

    for a in 0..AmbitionId::COUNT {
        for i in 0..s.declared[a].len() {
            s.available_markers.push(s.declared[a].as_slice()[i]);
        }
        s.declared[a].clear();
    }
    s.available_markers.as_mut_slice().sort_unstable();
    flip_lowest_unflipped(s, v);
    s.stats.chapters += 1;

    let done = (0..s.players as usize).any(|p| s.player_states[p].power >= v.power_threshold)
        || s.chapter >= v.max_chapters;
    if done {
        s.phase = Phase::Over;
        return;
    }
    s.chapter += 1;
    s.phase = Phase::Deal;
}

/// Trophies go home: pieces to their supplies, cities back onto their
/// board. (`returnTrophies` in game.ts.)
fn return_trophies(s: &mut GameState) {
    for p in 0..s.players as usize {
        let trophies = s.player_states[p].trophies;
        s.player_states[p].trophies = [[0; TrophyKind::COUNT]; crate::state::MAX_SEATS];
        for (oi, counts) in trophies.iter().enumerate() {
            let cities = counts[TrophyKind::City.as_index()];
            {
                let owner = &mut s.player_states[oi];
                owner.ships_supply += counts[TrophyKind::Ship.as_index()];
                owner.starports_supply += counts[TrophyKind::Starport.as_index()];
                owner.agents_supply += counts[TrophyKind::Agent.as_index()];
                owner.cities_used = owner.cities_used.saturating_sub(cities);
            }
            if cities > 0 {
                // The returning cities cover resource slots again.
                s.compact_resources_of(Player(oi as u8));
            }
        }
    }
}

/// Captured agents go back to their owners' supplies.
/// (`returnCaptives` in game.ts.)
fn return_captives(s: &mut GameState) {
    for p in 0..s.players as usize {
        let captives = s.player_states[p].captives;
        s.player_states[p].captives = [0; crate::state::MAX_SEATS];
        for (oi, n) in captives.iter().enumerate() {
            s.player_states[oi].agents_supply += n;
        }
    }
}

// ---------------------------------------------------------------------------
// Results
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Standing {
    pub player: Player,
    pub power: u8,
    pub rank: u8,
}

/// Final standings. "On a tie, the tied player earliest in turn order is the
/// winner" (p19) — turn order runs from the initiative holder clockwise.
/// (`standings` in game.ts.)
pub fn standings(s: &GameState) -> InlineVec<Standing, 4> {
    let order: InlineVec<Player, 4> = (0..s.players)
        .map(|i| Player((s.initiative.0 + i) % s.players))
        .collect();
    let mut sorted = order;
    sorted.as_mut_slice().sort_by(|a, b| {
        s.player(*b)
            .power
            .cmp(&s.player(*a).power)
            .then_with(|| order.position(a).cmp(&order.position(b)))
    });
    sorted
        .iter()
        .enumerate()
        .map(|(rank, &player)| Standing {
            player,
            power: s.player(player).power,
            rank: rank as u8,
        })
        .collect()
}

pub fn winner(s: &GameState) -> Player {
    standings(s).as_slice()[0].player
}

// ---------------------------------------------------------------------------
// Setup (`newGame` in setup.ts)
// ---------------------------------------------------------------------------

/// Deal the opening position (p4-p5). The chapter's hands are dealt by the
/// `deal` chance node, so `new_game` leaves the game there.
///
/// **Which player takes which position on the card follows the initiative
/// marker** (p5 step N): initiative goes to a random player, and positions
/// rotate clockwise from that seat — a random *rotation*, not a random
/// permutation, and the player who opens on position 1 always moves first.
pub fn new_game(
    v: &VariantDef,
    rng: &mut impl Rng,
    setup_index: u64,
    mode: SetupMode,
) -> GameState {
    let setup = draw_setup(v.players, setup_index, mode);
    let dead = |cluster: u8| setup.out_of_play.contains(&cluster);

    let mut systems: [SystemState; SYSTEM_COUNT] = core::array::from_fn(|i| SystemState {
        out_of_play: dead(v.systems[i].cluster),
        ..SystemState::default()
    });
    let mut player_states = [PlayerState::new(); crate::state::MAX_SEATS];

    // 5 tokens of each resource type in the general supply (p3).
    let mut supply = [5u8; ResourceType::COUNT];

    // p4 step B: the initiative marker goes to a random player, which also
    // decides who opens on which of the card's positions.
    let initiative = Player(rng.gen_range(v.players as usize) as u8);

    // p5 step N: 3 ships + city in A, 3 ships + starport in B, 2 ships in
    // each C.
    for (position, start) in setup.starts.iter().enumerate() {
        let p = Player((initiative.0 + position as u8) % v.players);
        let pi = p.as_index();
        let mut place = |system: crate::types::SystemId, ships: u8| {
            systems[system.as_index()].fresh[pi] += ships;
            player_states[pi].ships_supply -= ships;
        };
        place(start.a, 3);
        place(start.b, 3);
        for &c in start.c.iter() {
            place(c, 2);
        }
        systems[start.a.as_index()]
            .buildings
            .push(Building::new(p, BuildingKind::City, false));
        player_states[pi].cities_used += 1;
        systems[start.b.as_index()]
            .buildings
            .push(Building::new(p, BuildingKind::Starport, false));
        player_states[pi].starports_supply -= 1;

        // p5 step O: gain the resources matching the A and B planet types.
        for system in [start.a, start.b] {
            if let Some(t) = v.systems[system.as_index()].planet_type
                && supply[t.as_index()] > 0
                && player_states[pi].gain_resource(t)
            {
                supply[t.as_index()] -= 1;
            }
        }
    }

    // p4 step K: at 2 players the covered planets' resources become a
    // phantom rival.
    let mut phantom = [0u8; AmbitionId::COUNT];
    if v.players == 2 {
        for &cluster in setup.out_of_play.iter() {
            for p in 0..3u8 {
                let Some(t) = v.systems[planet_id(cluster, p).as_index()].planet_type else {
                    continue;
                };
                if supply[t.as_index()] == 0 {
                    continue;
                }
                supply[t.as_index()] -= 1;
                match t {
                    ResourceType::Material | ResourceType::Fuel => {
                        phantom[AmbitionId::Tycoon.as_index()] += 1
                    }
                    ResourceType::Weapon => phantom[AmbitionId::Warlord.as_index()] += 1,
                    ResourceType::Relic => phantom[AmbitionId::Keeper.as_index()] += 1,
                    ResourceType::Psionic => phantom[AmbitionId::Empath.as_index()] += 1,
                }
            }
        }
    }

    // p4 step H: shuffle the Court deck and fill the row from the top (the
    // deck's top is the InlineVec's end, as in the TS `pop()`).
    let mut court_deck = v.court_deck;
    rng.shuffle(&mut court_deck);
    let mut court_deck: InlineVec<crate::types::CourtCardId, { crate::court::COURT_CARD_COUNT }> =
        court_deck.iter().copied().collect();
    let mut court = InlineVec::new();
    for _ in 0..v.court_row_size {
        court.push(CourtSlot {
            card: court_deck.pop().expect("court deck exhausted at setup"),
            agents: [0; crate::state::MAX_SEATS],
        });
    }

    GameState {
        players: v.players,
        chapter: 1,
        phase: Phase::Deal,
        initiative,
        initiative_seized: false,
        systems,
        player_states,
        supply,
        cartel: [0; ResourceType::COUNT],
        court,
        court_deck,
        court_discard: InlineVec::new(),
        action_deck: v.action_deck,
        action_discard: InlineVec::new(),
        round: RoundState::default(),
        turn: None,
        battle: None,
        moving: None,
        declared: Default::default(),
        available_markers: (0..v.ambition_markers.len() as u8).collect(),
        flipped: [false; crate::ambitions::AMBITION_MARKER_COUNT],
        phantom,
        reinforcing: None,
        unions: InlineVec::new(),
        pending_vox: None,
        peek: None,
        revealed: InlineVec::new(),
        declines: InlineVec::new(),
        stats: GameStats::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ActionKind;

    /// The "free grants before pips, most-restrictive first" engine ruling
    /// (game.ts header) — pinned now so R2's board actions inherit it.
    #[test]
    fn pay_for_prefers_the_most_restrictive_free_grant() {
        let mut turn = TurnState {
            player: Player(0),
            mode: PlayMode::Lead,
            card: ActionCardId(0),
            pips_left: 2,
            pip_actions: ActionKindSet::from_kinds(&[ActionKind::Build, ActionKind::Repair]),
            free_actions: InlineVec::from_slice(&[
                ActionKindSet::from_kinds(&[ActionKind::Build, ActionKind::Repair]),
                ActionKindSet::from_kinds(&[ActionKind::Repair]),
            ]),
            weapon_spent: false,
            prelude_over: true,
            declared_this_turn: false,
            prelude_spent: [0; ResourceType::COUNT],
            secured_this_prelude: InlineVec::new(),
            card_preludes_used: InlineVec::new(),
        };
        // Repair consumes the single-kind grant, not the wide one, and not
        // a pip.
        pay_for(&mut turn, ActionKind::Repair).unwrap();
        assert_eq!(turn.pips_left, 2);
        assert_eq!(turn.free_actions.len(), 1);
        assert_eq!(turn.free_actions.as_slice()[0].len(), 2);
        // Build consumes the wide grant next.
        pay_for(&mut turn, ActionKind::Build).unwrap();
        assert_eq!(turn.pips_left, 2);
        assert!(turn.free_actions.is_empty());
        // Then pips; Battle only with a Weapon spent.
        pay_for(&mut turn, ActionKind::Build).unwrap();
        assert_eq!(turn.pips_left, 1);
        assert!(pay_for(&mut turn, ActionKind::Battle).is_err());
        turn.weapon_spent = true;
        pay_for(&mut turn, ActionKind::Battle).unwrap();
        assert_eq!(turn.pips_left, 0);
        assert!(pay_for(&mut turn, ActionKind::Build).is_err());
    }
}
