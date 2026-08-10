//! Guild and Vox card ability dispatch (rulebook p20), mirroring
//! `src/engine/powers.ts`.
//!
//! The rulebook sorts card powers into kinds, and this module handles each
//! where the state machine reaches for it:
//!
//! - **`Prelude:` abilities** — enumerated in the prelude phase, most of them
//!   discarding the card to do something once.
//! - **New actions** written `Name (Standard):` — offered wherever the
//!   standard action they replace is affordable, and paid for the same way.
//! - **Passive modifiers** — queried by the engine at the point they apply
//!   (extra battle dice, theft immunity, the zero marker, spending a resource
//!   as another type, rerolling skirmish dice).
//! - **Vox `When Secured:`** — resolved through `s.pending_vox`, since
//!   securing happens mid-turn or mid-battle and that flow has to resume
//!   afterwards.
//!
//! Enumeration and application are deliberately kept next to each other for
//! each ability: every `legal*` list here is the exact set `apply*` accepts,
//! which is what lets the agents assert they never play an unoffered action.
//!
//! **The expansion seam** (plan §4): every power consultation goes through
//! [`ability_sources`], the one place that knows where abilities come from —
//! held Guild cards today, the `leader` / `lore` fields on
//! [`crate::state::PlayerState`] once Leaders & Lore lands.

use crate::action::{
    Action, CardActionChoice, CardActionName, CardList, PreludeChoice, ResourceList, VoxChoice,
};
use crate::cards::action_card;
use crate::court::{
    CardPower, CourtCardKind, NewAction, NewActionEffect, NoZeroScope, Passive, PreludeAbility,
    VoxEffect, court_card,
};
use crate::game::RuleError;
use crate::inline_vec::InlineVec;
use crate::map::{CLUSTER_COUNT, SYSTEM_COUNT, SystemKind, cluster_of, is_gate};
use crate::setup::VariantDef;
use crate::state::{ActionKindSet, GameState, MAX_SEATS, PlayerState, TrophyKind, UnionAttachment};
use crate::types::{
    ActionKind, AmbitionId, ByResource, CourtCardId, PlayMode, Player, ResourceType, SystemId,
};

/// One source of an engine-readable card power, and the Guild card carrying
/// it — `card` is `None` for leader and lore powers (Leaders & Lore, R4+),
/// which are permanent and never discarded on use.
#[derive(Clone, Copy, Debug)]
pub struct AbilitySource {
    pub card: Option<CourtCardId>,
    pub power: &'static CardPower,
}

/// Every ability source a player currently has (`poweredCards` in powers.ts,
/// widened per the plan). Held Guild cards now; `leader` / `lore` later —
/// the fields already exist on [`PlayerState`].
pub fn ability_sources(s: &GameState, p: Player) -> impl Iterator<Item = AbilitySource> + '_ {
    s.player(p).guild_cards.iter().filter_map(|&id| {
        court_card(id).power.as_ref().map(|power| AbilitySource {
            card: Some(id),
            power,
        })
    })
}

fn passives(
    s: &GameState,
    p: Player,
) -> impl Iterator<Item = (Option<CourtCardId>, &'static Passive)> + '_ {
    ability_sources(s, p).flat_map(|src| src.power.passives.iter().map(move |x| (src.card, x)))
}

/// The resource types a Loyal card lets this player treat other resources
/// as. (`loyalTypes` in powers.ts.)
pub fn loyal_types(s: &GameState, p: Player) -> InlineVec<ResourceType, { ResourceType::COUNT }> {
    let mut out = InlineVec::new();
    for (_, passive) in passives(s, p) {
        if let Passive::Loyal { spend_as } = passive {
            out.push(*spend_as);
        }
    }
    out
}

/// "If you Provoke Outrage, keep this card" — a Loyal card survives the
/// Outrage discard of its own suit (p16 vs the Loyal cards' text).
/// (`survivesOutrage` in powers.ts.)
pub fn survives_outrage(card: CourtCardId) -> bool {
    court_card(card)
        .power
        .map(|p| {
            p.passives
                .iter()
                .any(|x| matches!(x, Passive::Loyal { .. }))
        })
        .unwrap_or(false)
}

/// Extra battle dice from passives, given where the battle is (Gatekeepers).
/// (`extraBattleDice` in powers.ts.)
pub fn extra_battle_dice(s: &GameState, p: Player, system: SystemId) -> u8 {
    if !is_gate(system) {
        return 0;
    }
    let mut extra = 0;
    for (_, passive) in passives(s, p) {
        if let Passive::GateDice { count } = passive {
            extra += count;
        }
    }
    extra
}

/// Sworn Guardians: Rivals cannot steal your resources and other Guild
/// cards. (`theftImmune` in powers.ts.)
pub fn theft_immune(s: &GameState, p: Player) -> bool {
    passives(s, p).any(|(_, x)| matches!(x, Passive::TheftImmune))
}

/// The one card a raider can still take from a theft-immune player.
/// (`theftImmunityCard` in powers.ts.)
pub fn theft_immunity_card(s: &GameState, p: Player) -> Option<CourtCardId> {
    passives(s, p)
        .find(|(_, x)| matches!(x, Passive::TheftImmune))
        .and_then(|(card, _)| card)
}

/// Whether declaring skips the zero marker.
///
/// Secret Order exempts Keeper and Empath; Galactic Bards exempts the
/// ambition it lets you declare on a Surpass or Pivot.
/// (`skipsZeroMarker` in powers.ts.)
pub fn skips_zero_marker(s: &GameState, p: Player, ambition: AmbitionId, mode: PlayMode) -> bool {
    for (_, passive) in passives(s, p) {
        let Passive::NoZeroMarker { ambitions } = passive else {
            continue;
        };
        match ambitions {
            NoZeroScope::KeeperEmpath
                if ambition == AmbitionId::Keeper || ambition == AmbitionId::Empath =>
            {
                return true;
            }
            NoZeroScope::SurpassPivot if mode == PlayMode::Surpass || mode == PlayMode::Pivot => {
                return true;
            }
            _ => {}
        }
    }
    false
}

/// "Your total Weapon icons from resources and cards" — the size gate on
/// Skirmishers' reroll and on Court Enforcers' Abduct.
/// (`weaponIcons` in powers.ts.)
pub fn weapon_icons(p: &PlayerState) -> u8 {
    let mut n = p
        .resources
        .iter()
        .flatten()
        .filter(|&&r| r == ResourceType::Weapon)
        .count() as u8;
    for &id in p.guild_cards.iter() {
        if court_card(id).suit == Some(ResourceType::Weapon) {
            n += 1;
        }
    }
    n
}

/// Skirmishers: may reroll blank skirmish dice after rolling.
/// (`canRerollSkirmish` in powers.ts.)
pub fn can_reroll_skirmish(s: &GameState, p: Player) -> bool {
    passives(s, p).any(|(_, x)| matches!(x, Passive::RerollSkirmish))
}

/// Farseers: on declaring an ambition, look at a Rival hand and may swap.
/// (`canPeek` in powers.ts.)
pub fn can_peek(s: &GameState, p: Player) -> bool {
    passives(s, p).any(|(_, x)| matches!(x, Passive::PeekAndSwap))
}

/// Galactic Bards: may declare on a Surpass or Pivot if nobody has yet.
/// (`canDeclareOnFollow` in powers.ts.)
pub fn can_declare_on_follow(s: &GameState, p: Player) -> bool {
    passives(s, p).any(|(_, x)| {
        matches!(
            x,
            Passive::NoZeroMarker {
                ambitions: NoZeroScope::SurpassPivot
            }
        )
    })
}

// ---------------------------------------------------------------------------
// Cartels
// ---------------------------------------------------------------------------

/// Which player, if any, holds the Cartel card for this resource type.
///
/// A Cartel keeps its type's whole supply on the card — "you add it to
/// Tycoon but can't spend it" (p20). (`cartelHolder` in board.ts.)
pub fn cartel_holder(s: &GameState, r: ResourceType) -> Option<Player> {
    for p in 0..s.players {
        let p = Player(p);
        for (_, passive) in passives(s, p) {
            if matches!(passive, Passive::Cartel { resource } if *resource == r) {
                return Some(p);
            }
        }
    }
    None
}

/// Move a resource type's whole supply onto a newly acquired Cartel card.
/// (`collectCartelSupply` in board.ts.)
pub fn collect_cartel_supply(s: &mut GameState, r: ResourceType) {
    s.cartel[r.as_index()] += s.supply[r.as_index()];
    s.supply[r.as_index()] = 0;
}

/// Release a Cartel's held supply back to the general supply.
/// (`releaseCartelSupply` in board.ts.)
pub fn release_cartel_supply(s: &mut GameState, r: ResourceType) {
    s.supply[r.as_index()] += s.cartel[r.as_index()];
    s.cartel[r.as_index()] = 0;
}

/// Resources a player holds on Cartel cards, which count toward Tycoon.
/// (`cartelIcons` in powers.ts.)
pub fn cartel_icons(s: &GameState, player: Player) -> ByResource<u8> {
    let mut out = [0u8; ResourceType::COUNT];
    for r in ResourceType::ALL {
        if cartel_holder(s, r) == Some(player) {
            out[r.as_index()] = s.cartel[r.as_index()];
        }
    }
    out
}

/// "After scoring, Rivals discard all Material / Fuel" — the two Cartel
/// cards (p20). The tokens go onto the Cartel card, which is where that
/// type's supply lives while the card is in play.
/// (`cartelSqueeze` in game.ts.)
pub fn cartel_squeeze(s: &mut GameState) {
    for r in ResourceType::ALL {
        let Some(holder) = cartel_holder(s, r) else {
            continue;
        };
        for p in 0..s.players as usize {
            if p == holder.as_index() {
                continue;
            }
            for slot in 0..s.player_states[p].resources.len() {
                if s.player_states[p].resources[slot] == Some(r) {
                    s.player_states[p].resources[slot] = None;
                    s.cartel[r.as_index()] += 1;
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Gaining and losing Guild cards
// ---------------------------------------------------------------------------

/// Give a Guild card to a player, collecting a Cartel's supply onto it.
///
/// Every route into a player's play area goes through here — securing,
/// raiding, Silver-Tongues, Guild Struggle — so a Cartel cannot arrive
/// without taking its supply with it. (`gainGuildCard` in powers.ts.)
pub fn gain_guild_card(s: &mut GameState, player: Player, card: CourtCardId) {
    s.player_mut(player).guild_cards.push(card);
    for passive in court_card(card).power.map(|p| p.passives).unwrap_or(&[]) {
        if let Passive::Cartel { resource } = passive {
            collect_cartel_supply(s, *resource);
        }
    }
}

/// Take a Guild card off a player, without deciding where it goes next.
/// (`removeGuildCard` in powers.ts.)
pub fn remove_guild_card(s: &mut GameState, player: Player, card: CourtCardId) {
    let held = &mut s.player_mut(player).guild_cards;
    if let Some(i) = held.position(&card) {
        held.remove(i);
    }
}

/// Discard a Guild card out of play, releasing a Cartel's held supply.
///
/// Engine ruling: the cards do not say what happens to a Cartel's stockpile
/// when the card leaves play. Returning it to the general supply is the only
/// reading that keeps the tokens in the game.
/// (`discardGuildCard` in powers.ts.)
pub fn discard_guild_card(s: &mut GameState, player: Player, card: CourtCardId) {
    remove_guild_card(s, player, card);
    for passive in court_card(card).power.map(|p| p.passives).unwrap_or(&[]) {
        if let Passive::Cartel { resource } = passive {
            release_cartel_supply(s, *resource);
        }
    }
    s.court_discard.push(card);
}

/// Move a Guild card between players (raids, Silver-Tongues, Guild
/// Struggle). (`stealGuildCard` in powers.ts.)
pub fn steal_guild_card(s: &mut GameState, from: Player, to: Player, card: CourtCardId) {
    remove_guild_card(s, from, card);
    s.player_mut(to).guild_cards.push(card);
    // The Cartel's supply travels on the card, so nothing moves between
    // pools.
}

// ---------------------------------------------------------------------------
// Outrage
// ---------------------------------------------------------------------------

/// Provoke Outrage of one type against one player (p16): discard every
/// resource and Guild card of that type, flip the Outrage marker, and lose
/// an agent. Used both when a city is destroyed and by the Vox card Outrage
/// Spreads. (`provokeOutrage` in powers.ts.)
pub fn provoke_outrage(s: &mut GameState, player: Player, r: ResourceType) {
    let pi = player.as_index();
    for slot in 0..s.player_states[pi].resources.len() {
        if s.player_states[pi].resources[slot] == Some(r) {
            s.player_states[pi].resources[slot] = None;
            s.return_to_supply(r);
        }
    }
    let held = s.player_states[pi].guild_cards;
    for &card in held.iter() {
        // "If you Provoke Outrage, keep this card" — the Loyal cards (p20).
        if court_card(card).suit == Some(r) && !survives_outrage(card) {
            discard_guild_card(s, player, card);
        }
    }
    let p = &mut s.player_states[pi];
    if !p.outrage[r.as_index()] {
        p.outrage[r.as_index()] = true;
        if p.agents_supply > 0 {
            p.agents_supply -= 1;
        }
    }
}

// ---------------------------------------------------------------------------
// Prelude abilities
// ---------------------------------------------------------------------------

/// Systems a "place N ships" Prelude may target: ones the player controls.
/// (`controlledSystems` in powers.ts.)
pub fn controlled_systems(s: &GameState, player: Player) -> InlineVec<SystemId, SYSTEM_COUNT> {
    let mut out = InlineVec::new();
    for i in 0..SYSTEM_COUNT {
        let system = SystemId(i as u8);
        if s.systems[i].out_of_play {
            continue;
        }
        if s.control_of(system) == Some(player) {
            out.push(system);
        }
    }
    out
}

/// A rival resource slot a steal ability can target.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
struct StealTarget {
    player: Player,
    slot: u8,
}

/// Rival resource slots a steal ability can target. `want` of `None` means
/// any type. (`stealTargets` in powers.ts.)
fn steal_targets(
    s: &GameState,
    player: Player,
    want: Option<ResourceType>,
) -> InlineVec<StealTarget, { MAX_SEATS * crate::player_board::MAX_RESOURCE_SLOTS }> {
    let mut out = InlineVec::new();
    for p in 0..s.players {
        let p = Player(p);
        if p == player {
            continue;
        }
        if theft_immune(s, p) {
            continue;
        }
        let victim = s.player(p);
        for slot in 0..victim.open_resource_slots() {
            let Some(r) = victim.resources[slot] else {
                continue;
            };
            if want.is_some_and(|w| w != r) {
                continue;
            }
            out.push(StealTarget {
                player: p,
                slot: slot as u8,
            });
        }
    }
    out
}

/// A rival Guild card a steal ability can target.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
struct StealCardTarget {
    player: Player,
    card: CourtCardId,
}

/// Rival Guild cards a steal ability can target.
///
/// Sworn Guardians shields the rest of its owner's cards but not itself —
/// "in battle they can steal this first and then spend keys" — and the same
/// reading applies to Silver-Tongues and Guild Struggle, which take exactly
/// one card. (`stealCardTargets` in powers.ts.)
fn steal_card_targets(
    s: &GameState,
    player: Player,
) -> InlineVec<StealCardTarget, { crate::court::GUILD_CARD_COUNT }> {
    let mut out = InlineVec::new();
    for p in 0..s.players {
        let p = Player(p);
        if p == player {
            continue;
        }
        if theft_immune(s, p) {
            if let Some(shield) = theft_immunity_card(s, p) {
                out.push(StealCardTarget {
                    player: p,
                    card: shield,
                });
            }
            continue;
        }
        for &card in s.player(p).guild_cards.iter() {
            out.push(StealCardTarget { player: p, card });
        }
    }
    out
}

/// Slots a player can still fill — free of both tokens and unplaced ones, so
/// a multi-token gain cannot promise more room than it has.
fn empty_slots(p: &PlayerState) -> usize {
    p.free_slots()
}

/// A `cardPrelude` action naming a choice.
const fn prelude(card: CourtCardId, choice: PreludeChoice) -> Action {
    Action::CardPrelude { card, choice }
}

/// Enumerate the legal `cardPrelude` actions for the player on turn.
/// (`preludeCardActions` in powers.ts.)
pub fn prelude_card_actions(s: &GameState, _v: &VariantDef, player: Player, out: &mut Vec<Action>) {
    let Some(turn) = s.turn else { return };
    let p = s.player(player);

    for src in ability_sources(s, player) {
        let Some(card) = src.card else { continue };
        let Some(ability) = src.power.prelude else {
            continue;
        };
        // "You cannot use Prelude actions on cards that you secured from the
        // Court in the same Prelude." (p20)
        if turn.secured_this_prelude.contains(&card) {
            continue;
        }
        if turn.card_preludes_used.contains(&card) {
            continue;
        }

        match ability {
            PreludeAbility::PlaceShips { .. } => {
                if p.ships_supply == 0 {
                    continue;
                }
                for &system in controlled_systems(s, player).iter() {
                    out.push(prelude(card, PreludeChoice::System(system)));
                }
            }
            PreludeAbility::ShipInEveryGate => {
                if p.ships_supply > 0 {
                    out.push(prelude(card, PreludeChoice::Bare));
                }
            }
            PreludeAbility::FillSlots { .. } => {
                if empty_slots(p) > 0 {
                    out.push(prelude(card, PreludeChoice::Bare));
                }
            }
            PreludeAbility::GainResources { .. } => {
                if empty_slots(p) > 0 {
                    out.push(prelude(card, PreludeChoice::Bare));
                }
            }
            PreludeAbility::SeizeInitiative => {
                if !s.initiative_seized && player != s.initiative {
                    out.push(prelude(card, PreludeChoice::Bare));
                }
            }
            PreludeAbility::StealResource { resource } => {
                for t in steal_targets(s, player, Some(resource)).iter() {
                    out.push(prelude(
                        card,
                        PreludeChoice::StealResource {
                            target: t.player,
                            slot: t.slot,
                        },
                    ));
                }
            }
            PreludeAbility::StealAny => {
                // Silver-Tongues: "steal a Guild card or resource".
                for t in steal_targets(s, player, None).iter() {
                    out.push(prelude(
                        card,
                        PreludeChoice::StealResource {
                            target: t.player,
                            slot: t.slot,
                        },
                    ));
                }
                for t in steal_card_targets(s, player).iter() {
                    out.push(prelude(
                        card,
                        PreludeChoice::StealCard {
                            target: t.player,
                            card: t.card,
                        },
                    ));
                }
            }
            PreludeAbility::ConvertResource { gain } => {
                // Relic Fence: discard 1 resource to gain 1 Relic, keeping
                // the card.
                if s.supply[gain.as_index()] == 0 {
                    continue;
                }
                for slot in 0..p.open_resource_slots() {
                    if p.resources[slot].is_some() {
                        out.push(prelude(
                            card,
                            PreludeChoice::ConvertResource { slot: slot as u8 },
                        ));
                    }
                }
            }
            PreludeAbility::AttachUnion { suit } => {
                // "place this card next to a face-up played <suit> card".
                // The TS action names the play by index; the Rust action
                // names the played card itself (unambiguous: a card is
                // played at most once per round).
                for play in s.round.played.iter() {
                    if play.face_down {
                        continue;
                    }
                    if action_card(play.card).suit == suit {
                        out.push(prelude(card, PreludeChoice::Union { played: play.card }));
                    }
                }
            }
            PreludeAbility::RecycleHand => {
                // Farseers: discard this plus any subset of the hand, redraw
                // as many. Hands are at most 6 cards (p5 step P), so this is
                // at most 64 options — the same order as the dice splits a
                // Battle already enumerates. Enumerating them all keeps
                // Farseers' choice the player's rather than an engine
                // ruling. (TS caps the mask at 8 hand cards; the Rust
                // `CardList` capacity of 7 is the effective cap here.)
                let hand = p.hand.as_slice();
                let n = hand.len().min(7);
                let mut picks: Vec<CardList> = Vec::with_capacity(1 << n);
                for mask in 0u32..(1 << n) {
                    let mut pick = CardList::new();
                    for (i, &c) in hand.iter().take(n).enumerate() {
                        if mask & (1 << i) != 0 {
                            pick.push(c);
                        }
                    }
                    picks.push(pick);
                }
                // Smallest subsets first, stable within a size (as in TS).
                picks.sort_by_key(|p| p.len());
                for cards in picks {
                    out.push(prelude(card, PreludeChoice::Recycle { cards }));
                }
            }
        }
    }
}

/// Apply a `cardPrelude` action. (`applyCardPrelude` in powers.ts.)
pub fn apply_card_prelude(s: &mut GameState, v: &VariantDef, a: Action) -> Result<(), RuleError> {
    let Action::CardPrelude { card, choice } = a else {
        return Err(RuleError::Illegal("not a cardPrelude action"));
    };
    let turn = s.turn.ok_or(RuleError::Illegal("no turn in progress"))?;
    let player = turn.player;
    let ability = court_card(card)
        .power
        .and_then(|p| p.prelude)
        .ok_or(RuleError::Illegal("card has no Prelude ability"))?;

    let mut discard = true;

    match ability {
        PreludeAbility::PlaceShips { count } => {
            let PreludeChoice::System(system) = choice else {
                return Err(RuleError::Illegal("placeShips needs a system"));
            };
            let n = count.min(s.player(player).ships_supply);
            s.systems[system.as_index()].fresh[player.as_index()] += n;
            s.player_mut(player).ships_supply -= n;
        }
        PreludeAbility::ShipInEveryGate => {
            for def in &v.systems {
                if def.kind != SystemKind::Gate || s.systems[def.id.as_index()].out_of_play {
                    continue;
                }
                if s.player(player).ships_supply == 0 {
                    break;
                }
                s.systems[def.id.as_index()].fresh[player.as_index()] += 1;
                s.player_mut(player).ships_supply -= 1;
            }
        }
        PreludeAbility::FillSlots { resource } => {
            // "gain X up to your number of empty resource slots. If the
            // supply empties, steal the X instead."
            let mut want = empty_slots(s.player(player));
            while want > 0 {
                if s.supply[resource.as_index()] > 0 {
                    if !s.take_from_supply(v, player, resource) {
                        break;
                    }
                } else {
                    let targets = steal_targets(s, player, Some(resource));
                    let Some(t) = targets.iter().next().copied() else {
                        break;
                    };
                    steal_into(s, v, player, t.player, t.slot as usize);
                }
                want -= 1;
            }
        }
        PreludeAbility::GainResources { resources } => {
            for &r in resources {
                s.take_from_supply(v, player, r);
            }
        }
        PreludeAbility::SeizeInitiative => {
            s.round.seized_by = Some(player);
            s.initiative_seized = true;
            s.stats.seizes += 1;
        }
        PreludeAbility::StealResource { .. } => {
            let PreludeChoice::StealResource { target, slot } = choice else {
                return Err(RuleError::Illegal("steal needs a target and slot"));
            };
            steal_into(s, v, player, target, slot as usize);
        }
        PreludeAbility::StealAny => match choice {
            PreludeChoice::StealCard {
                target,
                card: taken,
            } => steal_guild_card(s, target, player, taken),
            PreludeChoice::StealResource { target, slot } => {
                steal_into(s, v, player, target, slot as usize)
            }
            _ => return Err(RuleError::Illegal("steal needs a target")),
        },
        PreludeAbility::ConvertResource { gain } => {
            let PreludeChoice::ConvertResource { slot } = choice else {
                return Err(RuleError::Illegal("convert needs a slot"));
            };
            let slot = slot as usize;
            let given = s
                .player(player)
                .resources
                .get(slot)
                .copied()
                .flatten()
                .ok_or(RuleError::Illegal("no resource in that slot"))?;
            s.player_mut(player).resources[slot] = None;
            s.return_to_supply(given);
            s.take_from_supply(v, player, gain);
            // Relic Fence stays in play; it is once per turn instead.
            discard = false;
            if let Some(turn) = s.turn.as_mut() {
                turn.card_preludes_used.push(card);
            }
        }
        PreludeAbility::AttachUnion { .. } => {
            // The card leaves the play area and sits on the played card
            // until the round ends. It scores nothing while attached, which
            // costs the holder nothing: ambitions only score at a chapter
            // break, by which time every attachment has already resolved.
            let PreludeChoice::Union { played } = choice else {
                return Err(RuleError::Illegal("attach needs a played card"));
            };
            if !s.round.played.iter().any(|c| c.card == played) {
                return Err(RuleError::Illegal("no such played card"));
            }
            remove_guild_card(s, player, card);
            s.unions.push(UnionAttachment {
                card,
                player,
                target: played,
            });
            discard = false;
        }
        PreludeAbility::RecycleHand => {
            // "discard this and any number of cards from your hand. Draw the
            // same number of cards (including Farseers) from the bottom of
            // the action discard pile." The count includes Farseers itself,
            // so n discarded hand cards redraw n + 1.
            let PreludeChoice::Recycle { cards: give } = choice else {
                return Err(RuleError::Illegal("recycle needs a card list"));
            };
            for &c in give.iter() {
                let hand = &mut s.player_mut(player).hand;
                if let Some(i) = hand.position(&c) {
                    hand.remove(i);
                }
            }
            // Discarded cards go on top, so they cannot be immediately
            // redrawn.
            for &c in give.iter() {
                s.action_discard.push(c);
            }
            let want = give.len() + 1;
            for _ in 0..want {
                if s.action_discard.is_empty() {
                    break;
                }
                let next = s.action_discard.remove(0);
                s.player_mut(player).hand.push(next);
            }
        }
    }

    if discard {
        discard_guild_card(s, player, card);
    }
    Ok(())
}

/// Move one resource from a Rival's slot into the player's slots.
/// (`stealInto` in powers.ts.)
fn steal_into(s: &mut GameState, v: &VariantDef, player: Player, victim: Player, slot: usize) {
    let Some(&Some(r)) = s.player(victim).resources.get(slot) else {
        return;
    };
    s.player_mut(victim).resources[slot] = None;
    if !s.gain_into(v, player, r) {
        s.return_to_supply(r); // no room: the token goes back to the supply (p17)
    }
}

// ---------------------------------------------------------------------------
// New actions
// ---------------------------------------------------------------------------

/// A `cardAction` naming a choice.
const fn card_action(card: CourtCardId, choice: CardActionChoice) -> Action {
    Action::CardAction { card, choice }
}

/// New actions a player can afford right now, given the kinds they can pay
/// for. (`cardActions` in powers.ts.)
pub fn card_actions(
    s: &GameState,
    v: &VariantDef,
    player: Player,
    kinds: ActionKindSet,
    out: &mut Vec<Action>,
) {
    let p = s.player(player);
    for src in ability_sources(s, player) {
        let Some(card) = src.card else { continue };
        for na in src.power.new_actions {
            if !kinds.contains(na.replaces) {
                continue;
            }

            match na.effect {
                NewActionEffect::GainResource { resource } => {
                    // Only offer it when the resource can actually be taken.
                    if s.supply[resource.as_index()] == 0 {
                        continue;
                    }
                    if empty_slots(p) == 0 {
                        continue;
                    }
                    // The two gain-a-resource cards differ only by name.
                    let choice = match na.name {
                        CardActionName::Manufacture => CardActionChoice::Manufacture,
                        CardActionName::Synthesize => CardActionChoice::Synthesize,
                        _ => continue,
                    };
                    out.push(card_action(card, choice));
                }
                NewActionEffect::Pressgang => {
                    // "Return any number of your Captives to gain any 1
                    // resource for each."
                    let most = (p.captive_count() as usize).min(empty_slots(p));
                    let available: Vec<ResourceType> = ResourceType::ALL
                        .into_iter()
                        .filter(|r| s.supply[r.as_index()] > 0)
                        .collect();
                    for k in 1..=most {
                        for gain in multisets(&available, k) {
                            out.push(card_action(card, CardActionChoice::Pressgang { gain }));
                        }
                    }
                }
                NewActionEffect::Execute => {
                    // "Move any number of your Captives to your Trophies."
                    for count in 1..=p.captive_count() {
                        out.push(card_action(card, CardActionChoice::Execute { count }));
                    }
                }
                NewActionEffect::Abduct => {
                    // "Capture all Rival agents from a card in the Court
                    // that has fewer Rival agents than your total Weapon
                    // icons."
                    let icons = weapon_icons(p);
                    for (i, court_slot) in s.court.iter().enumerate() {
                        let rivals: u8 = court_slot
                            .agents
                            .iter()
                            .enumerate()
                            .take(s.players as usize)
                            .filter(|(pl, _)| *pl != player.as_index())
                            .map(|(_, &n)| n)
                            .sum();
                        if rivals > 0 && rivals < icons {
                            out.push(card_action(
                                card,
                                CardActionChoice::Abduct { slot: i as u8 },
                            ));
                        }
                    }
                }
                NewActionEffect::Trade => trade_actions(s, v, player, card, out),
            }
        }
    }
}

/// Elder Broker's Trade: "Choose a Rival city you control. Swap 1 resource
/// with that Rival — take a resource of that city type from them, and give
/// them a resource they don't have."
///
/// Engine ruling: Sworn Guardians does **not** block this. Its text is
/// "Rivals cannot steal your resources", and the rulebook uses *steal* as a
/// keyword — raids steal, Silver-Tongues steals. Trade says "swap", and
/// hands something back, so it is not a theft. (`tradeActions` in powers.ts.)
fn trade_actions(
    s: &GameState,
    v: &VariantDef,
    player: Player,
    card: CourtCardId,
    out: &mut Vec<Action>,
) {
    let me = s.player(player);
    for def in &v.systems {
        let st = &s.systems[def.id.as_index()];
        if st.out_of_play {
            continue;
        }
        let Some(planet_type) = def.planet_type else {
            continue;
        };
        if s.control_of(def.id) != Some(player) {
            continue;
        }
        for (building, b) in st.buildings.iter().enumerate() {
            if b.kind() != crate::types::BuildingKind::City || b.player() == player {
                continue;
            }
            let them = s.player(b.player());
            // Their resource of the city's type.
            let take_slot =
                (0..them.open_resource_slots()).find(|&i| them.resources[i] == Some(planet_type));
            let Some(take_slot) = take_slot else { continue };
            // One of mine of a type they do not hold.
            for give_slot in 0..me.open_resource_slots() {
                let Some(mine) = me.resources[give_slot] else {
                    continue;
                };
                if them.resources.iter().flatten().any(|&r| r == mine) {
                    continue;
                }
                out.push(card_action(
                    card,
                    CardActionChoice::Trade {
                        system: def.id,
                        building: building as u8,
                        slot: take_slot as u8,
                        give_slot: give_slot as u8,
                    },
                ));
            }
        }
    }
}

/// Every multiset of `k` items from `types`, as sorted lists.
///
/// Pressgang gains one resource per returned Captive, freely chosen, so the
/// option set really is the multisets. `k` is bounded by empty resource
/// slots (at most 6, usually 0-3), which keeps this small in practice.
/// (`multisets` in powers.ts.)
fn multisets(types: &[ResourceType], k: usize) -> Vec<ResourceList> {
    if k == 0 {
        return vec![ResourceList::new()];
    }
    let mut out = Vec::new();
    for i in 0..types.len() {
        for rest in multisets(&types[i..], k - 1) {
            let mut pick = ResourceList::new();
            pick.push(types[i]);
            for &r in rest.iter() {
                pick.push(r);
            }
            out.push(pick);
        }
    }
    out
}

/// The standard action a card action is paid for with.
/// (`cardActionCost` in powers.ts.)
pub fn card_action_cost(card: CourtCardId, name: CardActionName) -> Result<ActionKind, RuleError> {
    new_action(card, name).map(|na| na.replaces)
}

/// The card's printed entry for a named new action.
fn new_action(card: CourtCardId, name: CardActionName) -> Result<&'static NewAction, RuleError> {
    court_card(card)
        .power
        .map(|p| p.new_actions)
        .unwrap_or(&[])
        .iter()
        .find(|na| na.name == name)
        .ok_or(RuleError::Illegal("unknown card action"))
}

/// Apply a `cardAction`. (`applyCardAction` in powers.ts.)
pub fn apply_card_action(
    s: &mut GameState,
    v: &VariantDef,
    player: Player,
    a: Action,
) -> Result<(), RuleError> {
    let Action::CardAction { card, choice } = a else {
        return Err(RuleError::Illegal("not a cardAction"));
    };
    // The printed entry still gates the action: a choice naming an ability the
    // card does not have is illegal, not merely inert.
    let na = new_action(card, choice.name())?;

    match choice {
        CardActionChoice::Manufacture | CardActionChoice::Synthesize => {
            let NewActionEffect::GainResource { resource } = na.effect else {
                return Err(RuleError::Illegal("card action is not a gain"));
            };
            s.take_from_supply(v, player, resource);
            Ok(())
        }
        CardActionChoice::Pressgang { gain } => {
            // One Captive returns to its owner's supply per resource gained.
            for &r in gain.iter() {
                let Some(owner) = pop_captive(s, player) else {
                    break;
                };
                s.player_mut(owner).agents_supply += 1;
                s.take_from_supply(v, player, r);
            }
            Ok(())
        }
        CardActionChoice::Execute { count } => {
            // TS ruling: the Captives moved are taken in capture order,
            // which only decides whose supply an agent later returns to.
            // The Rust count-matrix has no capture order, so the port
            // resolves it deterministically: highest owner index first
            // (see state.rs module docs; documented deviation, same
            // counts).
            let n = count.min(s.player(player).captive_count());
            for _ in 0..n {
                let Some(owner) = pop_captive(s, player) else {
                    break;
                };
                s.player_mut(player).trophies[owner.as_index()][TrophyKind::Agent.as_index()] += 1;
            }
            Ok(())
        }
        CardActionChoice::Abduct { slot } => {
            let slot = slot as usize;
            if slot >= s.court.len() {
                return Err(RuleError::Illegal("no such court slot"));
            }
            for pl in 0..s.players as usize {
                if pl == player.as_index() {
                    continue;
                }
                let n = s.court.as_slice()[slot].agents[pl];
                s.player_mut(player).captives[pl] += n;
                s.court.as_mut_slice()[slot].agents[pl] = 0;
            }
            Ok(())
        }
        CardActionChoice::Trade {
            system,
            building,
            slot,
            give_slot,
        } => {
            let building = building as usize;
            let slot = slot as usize;
            let give_slot = give_slot as usize;
            let owner = s.systems[system.as_index()]
                .buildings
                .as_slice()
                .get(building)
                .ok_or(RuleError::Illegal("no such building"))?
                .player();
            let taken = s.player(owner).resources.get(slot).copied().flatten();
            let given = s.player(player).resources.get(give_slot).copied().flatten();
            let (Some(taken), Some(given)) = (taken, given) else {
                return Ok(()); // TS silently no-ops on empty slots
            };
            s.player_mut(owner).resources[slot] = Some(given);
            s.player_mut(player).resources[give_slot] = Some(taken);
            Ok(())
        }
    }
}

/// Remove one captive from `player`'s pool and name its owner — the Rust
/// stand-in for the TS `captives.pop()` (capture order): highest owner
/// index first, deterministically.
fn pop_captive(s: &mut GameState, player: Player) -> Option<Player> {
    let captives = &mut s.player_mut(player).captives;
    for owner in (0..MAX_SEATS).rev() {
        if captives[owner] > 0 {
            captives[owner] -= 1;
            return Some(Player(owner as u8));
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Farseers: look at a Rival hand and maybe swap
// ---------------------------------------------------------------------------

/// "When you declare an ambition, look at a Rival's hand. You may swap 1
/// card with them."
///
/// This is two decisions, not one, and deliberately so: choosing whom to
/// look at has to be committed *before* the hand is revealed, or the ability
/// would leak every Rival's hand instead of the one it names. `observe()`
/// (R4) reveals only the chosen target's hand, and only while the swap is
/// pending. (`peekTargetActions` in powers.ts.)
pub fn peek_target_actions(s: &GameState, player: Player, out: &mut Vec<Action>) {
    for p in 0..s.players {
        let p = Player(p);
        if p == player {
            continue;
        }
        if !s.player(p).hand.is_empty() {
            out.push(Action::PeekTarget { target: Some(p) });
        }
    }
    out.push(Action::PeekTarget { target: None });
}

/// (`peekSwapActions` in powers.ts.)
pub fn peek_swap_actions(s: &GameState, out: &mut Vec<Action>) {
    let peek = s.peek.expect("peekSwap without a peek");
    let target = peek.target.expect("peekSwap without a target");
    for &give in s.player(peek.player).hand.iter() {
        for &take in s.player(target).hand.iter() {
            out.push(Action::PeekSwap { give, take });
        }
    }
    out.push(Action::PeekSwapSkip);
}

/// (`applyPeekSwap` in powers.ts.)
pub fn apply_peek_swap(s: &mut GameState, a: Action) {
    let Action::PeekSwap { give, take } = a else {
        return;
    };
    let Some(peek) = s.peek else { return };
    let Some(target) = peek.target else { return };
    let (Some(i), Some(j)) = (
        s.player(peek.player).hand.position(&give),
        s.player(target).hand.position(&take),
    ) else {
        return;
    };
    s.player_mut(peek.player).hand.as_mut_slice()[i] = take;
    s.player_mut(target).hand.as_mut_slice()[j] = give;
}

// ---------------------------------------------------------------------------
// Vox `When Secured`
// ---------------------------------------------------------------------------

/// Does this Vox card need a decision, or can it resolve on the spot?
///
/// Call to Action just draws a card, so it resolves inline and never blocks
/// the turn; the other five each ask the securing player something.
/// (`voxNeedsDecision` in powers.ts.)
pub fn vox_needs_decision(card: CourtCardId) -> bool {
    court_card(card).vox != Some(VoxEffect::DrawFromDiscardBottom)
}

/// Resolve the Vox effects that need no decision.
/// (`applyVoxImmediate` in powers.ts.)
pub fn apply_vox_immediate(s: &mut GameState, player: Player, card: CourtCardId) {
    debug_assert_eq!(court_card(card).vox, Some(VoxEffect::DrawFromDiscardBottom));
    if !s.action_discard.is_empty() {
        let next = s.action_discard.remove(0);
        s.player_mut(player).hand.push(next);
    }
    s.court_discard.push(card);
}

/// Enumerate the choices for the pending Vox card.
/// (`voxActions` in powers.ts.)
pub fn vox_actions(s: &GameState, out: &mut Vec<Action>) {
    let pending = s.pending_vox.expect("voxActions without a pending Vox");
    let player = pending.player;
    let p = s.player(player);
    let effect = court_card(pending.card)
        .vox
        .expect("pending Vox on a non-Vox card");

    match effect {
        VoxEffect::ShipInEachSystemOfCluster => {
            // "Choose a cluster on the map. You place 1 ship in each system
            // of that cluster." Not optional, so only offer a skip when
            // nothing is possible.
            if p.ships_supply > 0 {
                for c in 0..CLUSTER_COUNT as u8 {
                    let any_in_play = (0..SYSTEM_COUNT)
                        .any(|i| !s.systems[i].out_of_play && cluster_of(SystemId(i as u8)) == c);
                    if any_in_play {
                        out.push(Action::Vox(VoxChoice::Cluster(c)));
                    }
                }
            }
        }
        VoxEffect::DeclareAnyAmbition => {
            if !s.available_markers.is_empty() {
                for ambition in AmbitionId::ALL {
                    out.push(Action::Vox(VoxChoice::Declare(ambition)));
                }
            }
        }
        VoxEffect::OutrageAll => {
            for resource in ResourceType::ALL {
                out.push(Action::Vox(VoxChoice::Outrage(resource)));
            }
        }
        VoxEffect::ReturnCityMaySeize => {
            for i in 0..SYSTEM_COUNT {
                let system = SystemId(i as u8);
                if s.systems[i].out_of_play {
                    continue;
                }
                if s.control_of(system) != Some(player) {
                    continue;
                }
                for (bi, b) in s.systems[i].buildings.iter().enumerate() {
                    if b.kind() != crate::types::BuildingKind::City {
                        continue;
                    }
                    for seize in [false, true] {
                        if seize && (s.initiative_seized || player == s.initiative) {
                            continue;
                        }
                        out.push(Action::Vox(VoxChoice::ReturnCity {
                            system,
                            building: bi as u8,
                            seize,
                        }));
                    }
                }
            }
        }
        VoxEffect::StealGuildCardAndRecycle => {
            for t in steal_card_targets(s, player).iter() {
                out.push(Action::Vox(VoxChoice::Steal {
                    target: t.player,
                    card: t.card,
                }));
            }
        }
        VoxEffect::DrawFromDiscardBottom => {}
    }

    // Mass Uprising is the one mandatory choice — "Choose a cluster ... You
    // place 1 ship in each system" — so it only allows declining when
    // nothing is legal. The other four all read "You may".
    if effect != VoxEffect::ShipInEachSystemOfCluster || out.is_empty() {
        out.push(Action::VoxSkip);
    }
}

/// Apply a Vox choice and put the card away. Returns the ambition Populist
/// Demands declared, if any — declaring lives in the state machine, because
/// it pulls a marker and can trigger Farseers.
///
/// Engine ruling: two of these say "shuffle this card into the Court deck"
/// and one says "shuffle all Guild cards from the Court discard pile into
/// the Court deck". Securing is a decision node with no RNG in scope by
/// design (see `secure_card`), so the cards go to the *bottom* of the deck
/// in a defined order instead. The Court deck's order is hidden information
/// either way, and `determinize()` (R4) reshuffles it for search.
/// (`applyVox` in powers.ts.)
pub fn apply_vox(s: &mut GameState, a: Action) -> Result<Option<AmbitionId>, RuleError> {
    let pending = s
        .pending_vox
        .ok_or(RuleError::Illegal("no Vox effect pending"))?;
    let player = pending.player;
    let effect = court_card(pending.card)
        .vox
        .ok_or(RuleError::Illegal("pending Vox on a non-Vox card"))?;
    // `None` is a decline (VoxSkip); every other action is out of place.
    let choice = match a {
        Action::VoxSkip => None,
        Action::Vox(choice) => Some(choice),
        _ => return Err(RuleError::Illegal("a Vox effect is pending")),
    };
    let mut declare = None;

    match effect {
        VoxEffect::ShipInEachSystemOfCluster => {
            if let Some(VoxChoice::Cluster(cluster)) = choice {
                for i in 0..SYSTEM_COUNT {
                    if s.systems[i].out_of_play || cluster_of(SystemId(i as u8)) != cluster {
                        continue;
                    }
                    if s.player(player).ships_supply == 0 {
                        break;
                    }
                    s.systems[i].fresh[player.as_index()] += 1;
                    s.player_mut(player).ships_supply -= 1;
                }
            }
            s.court_discard.push(pending.card);
        }
        VoxEffect::DeclareAnyAmbition => {
            if let Some(VoxChoice::Declare(ambition)) = choice {
                declare = Some(ambition);
            }
            s.court_discard.push(pending.card);
        }
        VoxEffect::OutrageAll => {
            // "Each player (even you) must Provoke Outrage of that type."
            if let Some(VoxChoice::Outrage(resource)) = choice {
                for pl in 0..s.players {
                    provoke_outrage(s, Player(pl), resource);
                }
            }
            s.court_discard.push(pending.card);
        }
        VoxEffect::ReturnCityMaySeize => {
            if let Some(VoxChoice::ReturnCity {
                system,
                building,
                seize,
            }) = choice
            {
                let st = &s.systems[system.as_index()];
                let b = st.buildings.as_slice().get(building as usize).copied();
                if let Some(b) = b
                    && b.kind() == crate::types::BuildingKind::City
                {
                    s.systems[system.as_index()]
                        .buildings
                        .remove(building as usize);
                    let owner = b.player();
                    {
                        let o = s.player_mut(owner);
                        o.cities_used = o.cities_used.saturating_sub(1);
                    }
                    s.compact_resources_of(owner);
                    if seize && !s.initiative_seized && player != s.initiative {
                        s.round.seized_by = Some(player);
                        s.initiative_seized = true;
                        s.stats.seizes += 1;
                    }
                }
            }
            // "Shuffle this card into the Court deck" — bottom, per the
            // ruling above (the deck's top is the InlineVec's end).
            s.court_deck.insert(0, pending.card);
        }
        VoxEffect::StealGuildCardAndRecycle => {
            if let Some(VoxChoice::Steal { target, card }) = choice {
                if !s.player(target).guild_cards.contains(&card) {
                    return Err(RuleError::Illegal("target does not hold that card"));
                }
                steal_guild_card(s, target, player, card);
            }
            // "Shuffle all Guild cards from the Court discard pile into the
            // Court deck" — to the bottom, preserving their order.
            let guild: InlineVec<CourtCardId, { crate::court::COURT_CARD_COUNT }> = s
                .court_discard
                .iter()
                .copied()
                .filter(|&id| court_card(id).kind == CourtCardKind::Guild)
                .collect();
            s.court_discard
                .retain(|&id| court_card(id).kind != CourtCardKind::Guild);
            for (i, &id) in guild.iter().enumerate() {
                s.court_deck.insert(i, id);
            }
            s.court_discard.push(pending.card);
        }
        VoxEffect::DrawFromDiscardBottom => {
            apply_vox_immediate(s, player, pending.card);
        }
    }

    s.pending_vox = None;
    Ok(declare)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::court::COURT_DECK;

    #[test]
    fn loyal_cards_survive_outrage() {
        for card in &COURT_DECK {
            let is_loyal = card
                .power
                .map(|p| {
                    p.passives
                        .iter()
                        .any(|x| matches!(x, Passive::Loyal { .. }))
                })
                .unwrap_or(false);
            assert_eq!(survives_outrage(card.id), is_loyal, "{}", card.name);
        }
        // The five printed Loyal cards.
        let loyal: Vec<&str> = COURT_DECK
            .iter()
            .filter(|c| survives_outrage(c.id))
            .map(|c| c.name)
            .collect();
        assert_eq!(
            loyal,
            [
                "Loyal Engineers",
                "Loyal Pilots",
                "Loyal Marines",
                "Loyal Empaths",
                "Loyal Keepers"
            ]
        );
    }

    #[test]
    fn multisets_enumerate_like_ts() {
        use ResourceType::{Fuel, Material};
        let picks = multisets(&[Material, Fuel], 2);
        let as_vecs: Vec<Vec<ResourceType>> =
            picks.iter().map(|p| p.iter().copied().collect()).collect();
        assert_eq!(
            as_vecs,
            vec![
                vec![Material, Material],
                vec![Material, Fuel],
                vec![Fuel, Fuel]
            ]
        );
    }
}
