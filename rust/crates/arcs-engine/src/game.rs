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
//! **R1 scope**: setup, deal, mulligan, the card-play trick, the Prelude
//! (resource spending), and the turn/round/chapter skeleton. Board actions
//! (`tax`/`build`/`move`/`battle`/...) exist in the `Action` type but return
//! [`RuleError::NotYetImplemented`] and are not enumerated — they land in
//! R2. Guild-card powers, Vox and Farseers land in R3; chapter *scoring* is
//! stubbed where marked `R2`.

use crate::action::{Action, FollowMode};
use crate::ambitions::{flip_lowest_unflipped, highest_available};
use crate::cards::{CardAmbition, action_card, suit_actions};
use crate::inline_vec::InlineVec;
use crate::map::{SYSTEM_COUNT, SystemKind, planet_id};
use crate::rng::Rng;
use crate::setup::{SetupMode, VariantDef, draw_setup};
use crate::state::{
    ActionKindSet, Building, CourtSlot, GameState, GameStats, PlayedCard, PlayerState, RoundState,
    SystemState, TurnState,
};
use crate::types::{ActionCardId, AmbitionId, BuildingKind, Phase, PlayMode, Player, ResourceType};

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
    /// The action's phase lands in a later milestone (R2: board actions and
    /// battle; R3: card powers, Vox, Farseers).
    NotYetImplemented(&'static str),
}

impl core::fmt::Display for RuleError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            RuleError::WrongPhase => write!(f, "action does not belong to this phase"),
            RuleError::Illegal(why) => write!(f, "illegal action: {why}"),
            RuleError::NotYetImplemented(what) => write!(f, "not yet implemented: {what}"),
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
pub fn legal_actions(s: &GameState, v: &VariantDef, out: &mut Vec<Action>) {
    out.clear();
    if s.pending_vox.is_some() {
        // R3: voxActions(s).
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
        Phase::Reinforce => {
            for def in &v.systems {
                if def.kind == SystemKind::Gate && !s.systems[def.id.as_index()].out_of_play {
                    out.push(Action::Reinforce { system: def.id });
                }
            }
        }
        // R2: catapult, battleReroll, battleAssign. R3: peekTarget, peekSwap.
        _ => {}
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
fn prelude_actions(s: &GameState, _v: &VariantDef, out: &mut Vec<Action>) {
    let turn = s.turn.expect("prelude without a turn");
    let p = s.player(turn.player);

    // Declaring and seizing are decided before any Prelude action (p20).
    if !turn.prelude_over && turn.prelude_spent.is_empty() {
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
        // R3: `SpendResourceAs` offers via Loyal Guild cards (loyalTypes).
    }

    // R3: preludeCardActions (Guild-card `Prelude:` abilities).
    out.push(Action::BeginActions);
}

/// Which ambitions the current turn may declare. The normal route is the
/// leader declaring the ambition printed on a face-up lead card (p9).
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
        // R3: Galactic Bards declares on a Surpass or Pivot
        // (canDeclareOnFollow) if no ambition is declared yet this round.
        PlayMode::Surpass | PlayMode::Pivot => return out,
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

/// The actions phase. R1 skeleton: only `EndTurn` is enumerated — the board
/// actions the pips could buy land in R2.
fn turn_actions(s: &GameState, _v: &VariantDef, out: &mut Vec<Action>) {
    debug_assert!(s.turn.is_some());
    // R2: tax / build / move / repair / influence / secure / battle,
    //     gated on `available_kinds` (pips + weapon_spent + free grants).
    // R3: cardActions (Guild-card new actions).
    out.push(Action::EndTurn);
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
        // R2: battle rolls (rollBattle / rerollSkirmish + applyBattleRollMut).
        Phase::BattleRoll => Err(RuleError::NotYetImplemented("battle rolls land in R2")),
        _ => Err(RuleError::WrongPhase),
    }
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
        // R3: resolveVox.
        return Err(RuleError::NotYetImplemented("Vox resolution lands in R3"));
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
            // R3: offerPeek (Farseers) after any declaration.
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
        // R3: Guild-card Prelude abilities (applyCardPrelude in powers.ts).
        Action::CardPrelude { .. } => Err(RuleError::NotYetImplemented(
            "guild-card preludes land in R3",
        )),
        // R3: Guild-card new actions (applyCardAction in powers.ts).
        Action::CardAction { .. } => Err(RuleError::NotYetImplemented(
            "guild-card actions land in R3",
        )),
        // R2: the board actions bought with pips and free grants.
        Action::Tax { .. }
        | Action::BuildShip { .. }
        | Action::BuildBuilding { .. }
        | Action::Move { .. }
        | Action::Catapult { .. }
        | Action::CatapultStop
        | Action::Repair { .. }
        | Action::Influence { .. }
        | Action::Secure { .. }
        | Action::Battle { .. } => Err(RuleError::NotYetImplemented("board actions land in R2")),
        // R2: battle resolution.
        Action::AssignSelf { .. }
        | Action::AssignHit { .. }
        | Action::RaidResource { .. }
        | Action::RaidCard { .. }
        | Action::RaidDone
        | Action::RerollSkirmish { .. } => Err(RuleError::NotYetImplemented(
            "battle resolution lands in R2",
        )),
        // R3: Farseers.
        Action::PeekTarget { .. } | Action::PeekSwap { .. } | Action::PeekSwapSkip => {
            Err(RuleError::NotYetImplemented("Farseers lands in R3"))
        }
        // R3: Vox `When Secured`.
        Action::Vox { .. } | Action::VoxSkip => {
            Err(RuleError::NotYetImplemented("Vox effects land in R3"))
        }
    }
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
        prelude_spent: InlineVec::new(),
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
    _mode: PlayMode,
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
    // 0." R3: Secret Order exempts Keeper and Empath; Galactic Bards exempts
    // its own Surpass/Pivot declaration (skipsZeroMarker in powers.ts).
    s.round.lead_number = 0;
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
    turn.prelude_spent.push(real);

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
        for i in 0..turn.prelude_spent.len() {
            s.return_to_supply(turn.prelude_spent.as_slice()[i]);
        }
        turn.prelude_spent.clear();
        turn.prelude_over = true;
    }
    s.turn = Some(turn);
}

/// Pay for one action. Free grants from resources are consumed before card
/// pips, most-restrictive first (engine ruling — see the game.ts header).
/// Unused until the board actions land in R2; the most-restrictive-first
/// ruling is pinned by a unit test below.
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

// --- turn / round / chapter flow -------------------------------------------

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

/// Discard the round's played cards. (`discardPlayed` in game.ts.)
fn discard_played(s: &mut GameState) {
    // R3: Union attachments first claim the card they are attached to — the
    // owner draws it back into hand and the Union card is discarded (p20).
    // No Union can exist before Guild cards are holdable, so R1 only
    // asserts the list is empty.
    debug_assert!(s.unions.is_empty(), "unions land in R3");

    for i in 0..s.round.played.len() {
        let c = s.round.played.as_slice()[i];
        s.action_discard.push(c.card);
        // Played face up: the whole table watched this card leave play.
        if !c.face_down {
            s.revealed.push(c.card);
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

    // R2: scoreChapter — score every declared ambition into Power, return
    //     Trophies when Warlord scores and Captives when Tyrant scores, and
    //     apply the Cartel squeeze.

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
            prelude_spent: InlineVec::new(),
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
