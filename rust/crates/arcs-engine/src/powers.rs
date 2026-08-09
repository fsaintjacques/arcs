//! Card-power hooks, mirroring `src/engine/powers.ts`.
//!
//! **R2 scope**: only the pieces the base action/battle machinery needs —
//! moving Guild cards between zones and provoking Outrage. Everything driven
//! by held-card *abilities* (Loyal spending, Cartel supplies, Gatekeepers
//! dice, Skirmishers rerolls, theft immunity, Vox effects, Farseers, ...)
//! lands in R3 behind the `ability_sources` iterator; the call sites are
//! marked `R3` where they diverge from the TS reference until then.

use crate::court::{Passive, court_card};
use crate::state::GameState;
use crate::types::{CourtCardId, Player, ResourceType};

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

/// Give a Guild card to a player. Every route into a play area goes through
/// here so a Cartel cannot arrive without its supply — the collection itself
/// is R3 (no Cartel passive is active before then).
/// (`gainGuildCard` in powers.ts.)
pub fn gain_guild_card(s: &mut GameState, player: Player, card: CourtCardId) {
    s.player_mut(player).guild_cards.push(card);
    // R3: collect a Cartel's supply onto the card (collectCartelSupply).
}

/// Take a Guild card off a player, without deciding where it goes next.
/// (`removeGuildCard` in powers.ts.)
pub fn remove_guild_card(s: &mut GameState, player: Player, card: CourtCardId) {
    let held = &mut s.player_mut(player).guild_cards;
    if let Some(i) = held.position(&card) {
        held.remove(i);
    }
}

/// Discard a Guild card out of play. (`discardGuildCard` in powers.ts.)
pub fn discard_guild_card(s: &mut GameState, player: Player, card: CourtCardId) {
    remove_guild_card(s, player, card);
    // R3: release a Cartel's held supply back to the general supply.
    s.court_discard.push(card);
}

/// Move a Guild card between players (raids; Silver-Tongues and Guild
/// Struggle in R3). (`stealGuildCard` in powers.ts.)
pub fn steal_guild_card(s: &mut GameState, from: Player, to: Player, card: CourtCardId) {
    remove_guild_card(s, from, card);
    s.player_mut(to).guild_cards.push(card);
    // A Cartel's supply travels on the card, so nothing moves between pools.
}

/// Provoke Outrage of one type against one player (p16): discard every
/// resource and Guild card of that type, flip the Outrage marker, and lose
/// an agent. Used when a city is destroyed (and by Outrage Spreads in R3).
/// (`provokeOutrage` in powers.ts.)
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
}
