//! The action deck (rulebook p3, p8-p10), mirroring `src/engine/cards.ts`.
//!
//! 4 suits x numbers 1-7 = 28 cards. With 2-3 players the "1" and "7" cards
//! are returned to the box, leaving 20 cards numbered 2-6.

use crate::inline_vec::InlineVec;
use crate::types::{ActionCardId, ActionKind, AmbitionId, Suit};

/// The ambition a card can declare when led (p9): none on a "1", the fixed
/// one on 2-6, any on a "7".
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum CardAmbition {
    None,
    Any,
    Some(AmbitionId),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ActionCardDef {
    pub id: ActionCardId,
    pub suit: Suit,
    /// 1..7. With 2-3 players only 2..6 are in the deck.
    pub number: u8,
    /// Actions granted when led or surpassed with.
    pub pips: u8,
}

impl ActionCardDef {
    /// The ambition this card can declare when led.
    #[inline]
    pub const fn ambition(&self) -> CardAmbition {
        ambition_by_number(self.number)
    }
}

/// A short name like "A3" (suit initial + number), as in the TS `cardName`.
impl core::fmt::Display for ActionCardDef {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let initial = match self.suit {
            Suit::Administration => 'A',
            Suit::Aggression => 'A',
            Suit::Construction => 'C',
            Suit::Mobilization => 'M',
        };
        write!(f, "{initial}{}", self.number)
    }
}

/// Pips are inversely related to the card number (p3, confirmed p8-p10).
#[inline]
pub const fn pips_by_number(number: u8) -> u8 {
    match number {
        1 | 2 => 4,
        3 | 4 => 3,
        5 | 6 => 2,
        _ => 1,
    }
}

/// The ambition a number can declare when led (p9).
#[inline]
pub const fn ambition_by_number(number: u8) -> CardAmbition {
    match number {
        1 => CardAmbition::None,
        2 => CardAmbition::Some(AmbitionId::Tycoon),
        3 => CardAmbition::Some(AmbitionId::Tyrant),
        4 => CardAmbition::Some(AmbitionId::Warlord),
        5 => CardAmbition::Some(AmbitionId::Keeper),
        6 => CardAmbition::Some(AmbitionId::Empath),
        _ => CardAmbition::Any,
    }
}

/// Which actions a suit's pips can buy (p8).
pub const fn suit_actions(suit: Suit) -> &'static [ActionKind] {
    use ActionKind::{Battle, Build, Influence, Move, Repair, Secure, Tax};
    match suit {
        Suit::Administration => &[Tax, Repair, Influence],
        Suit::Aggression => &[Battle, Move, Secure],
        Suit::Construction => &[Build, Repair],
        Suit::Mobilization => &[Move, Influence],
    }
}

pub const ACTION_CARD_COUNT: usize = Suit::COUNT * 7;

/// All 28 cards, id = suit_index * 7 + (number - 1).
pub const ALL_ACTION_CARDS: [ActionCardDef; ACTION_CARD_COUNT] = {
    let mut cards = [ActionCardDef {
        id: ActionCardId(0),
        suit: Suit::Administration,
        number: 1,
        pips: 4,
    }; ACTION_CARD_COUNT];
    let mut i = 0;
    while i < ACTION_CARD_COUNT {
        let number = (i % 7) as u8 + 1;
        cards[i] = ActionCardDef {
            id: ActionCardId(i as u8),
            suit: Suit::ALL[i / 7],
            number,
            pips: pips_by_number(number),
        };
        i += 1;
    }
    cards
};

#[inline]
pub const fn action_card(id: ActionCardId) -> &'static ActionCardDef {
    &ALL_ACTION_CARDS[id.0 as usize]
}

/// The deck for a player count: 2-6 always, plus 1s and 7s at 4 players
/// (p4).
pub fn action_deck_for(players: u8) -> InlineVec<ActionCardId, ACTION_CARD_COUNT> {
    ALL_ACTION_CARDS
        .iter()
        .filter(|c| (2..=6).contains(&c.number) || players == 4)
        .map(|c| c.id)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_encode_suit_and_number() {
        for card in &ALL_ACTION_CARDS {
            assert_eq!(
                card.id.0 as usize,
                card.suit.as_index() * 7 + (card.number as usize - 1)
            );
            assert_eq!(card.pips, pips_by_number(card.number));
        }
    }

    #[test]
    fn deck_sizes_by_player_count() {
        assert_eq!(action_deck_for(2).len(), 20);
        assert_eq!(action_deck_for(3).len(), 20);
        assert_eq!(action_deck_for(4).len(), 28);
        for id in action_deck_for(3).iter() {
            let n = action_card(*id).number;
            assert!((2..=6).contains(&n));
        }
    }
}
