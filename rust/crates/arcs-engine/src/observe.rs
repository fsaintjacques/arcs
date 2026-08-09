//! Imperfect information: what a player may legally see, and how to sample a
//! consistent full state from it. Ported from `src/engine/observe.ts`.
//!
//! Arcs hides two things (p22, "Private Information"): the action cards in
//! other players' hands, and cards played face down (Copy plays and cards
//! spent to seize the initiative). Deck order is hidden from everyone.
//!
//! [`observe`] blanks those out; [`determinize`] deals them back at random in
//! a way consistent with everything the observer has seen, which is what
//! determinized search (ISMCTS) needs.
//!
//! Two shape differences from the TS source, both forced by `GameState` being
//! a flat POD:
//!
//! - a hidden face-down play carries [`HIDDEN_CARD`] instead of the TS `-1`,
//!   since `ActionCardId` is unsigned;
//! - `hand_sizes` / `face_down_counts` are `MAX_SEATS`-wide arrays whose
//!   entries past `players` stay zero, matching every other per-seat array.
//!
//! `src/engine/belief.ts` is deliberately **not** ported. It is a measured
//! null on the TS side — `beliefEnabled` is `false`, the per-card decline
//! signal is 0.962 and sampled-hand recall is 37.18% weighted against 37.19%
//! uniform — so porting it would carry a second sampler for no measurable
//! gain. `GameState::declines` is still recorded, so a future (learned)
//! inference model can drop into [`determinize`] behind
//! [`DeterminizeOptions`] exactly as the TS `opts.belief` seam does.

use crate::cards::ACTION_CARD_COUNT;
use crate::court::COURT_CARD_COUNT;
use crate::inline_vec::InlineVec;
use crate::rng::Rng;
use crate::setup::VariantDef;
use crate::state::{GameState, MAX_SEATS};
use crate::types::{ActionCardId, CourtCardId, Player};

/// The card id standing in for a face-down play the observer may not see —
/// the TS `card: -1`. It is not a legal card id, so a world that still holds
/// one has not been determinized.
pub const HIDDEN_CARD: ActionCardId = ActionCardId(u8::MAX);

/// A legal view of the game for one player.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Observation {
    /// The observer.
    pub player: Player,
    /// State with hidden information replaced by counts.
    pub state: GameState,
    /// How many cards each player holds (own hand is exact in `state`).
    pub hand_sizes: [u8; MAX_SEATS],
    /// Cards the observer knows are face down in front of each player.
    pub face_down_counts: [u8; MAX_SEATS],
}

/// Whose hand this observer may currently see, courtesy of Farseers.
///
/// The reveal is exactly what the card grants — one named Rival's hand, and
/// only while the swap decision is open. Choosing the target is a separate,
/// earlier decision precisely so that this cannot leak more than one hand.
fn peeked_hand(s: &GameState, player: Player) -> Option<Player> {
    let peek = s.peek?;
    if peek.player != player {
        return None;
    }
    peek.target
}

pub fn observe(s: &GameState, _v: &VariantDef, player: Player) -> Observation {
    let mut view = *s;
    let peeked = peeked_hand(s, player);

    for p in (0..s.players).map(Player) {
        if p == player || Some(p) == peeked {
            continue;
        }
        view.player_mut(p).hand.clear();
    }
    // Deck and discard order are unknown to everyone. `revealed` survives: it
    // is the public record of which cards are known to be *in* that discard.
    view.action_deck.clear();
    view.action_discard.clear();
    view.court_deck.clear();
    // Face-down plays by others are hidden.
    for c in view.round.played.as_mut_slice() {
        if c.face_down && c.player != player {
            c.card = HIDDEN_CARD;
        }
    }

    let mut hand_sizes = [0u8; MAX_SEATS];
    for (p, size) in hand_sizes.iter_mut().enumerate().take(s.players as usize) {
        *size = s.player_states[p].hand.len() as u8;
    }
    let mut face_down_counts = [0u8; MAX_SEATS];
    for c in s.round.played.iter() {
        if c.face_down {
            face_down_counts[c.player.as_index()] += 1;
        }
    }

    Observation {
        player,
        state: view,
        hand_sizes,
        face_down_counts,
    }
}

/// Options for [`determinize`].
///
/// Empty for now: the TS `belief` switch is a measured null and is not ported
/// (see the module docs). The type exists so a future inference model becomes
/// a field rather than a signature change across every caller and FFI
/// binding.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
#[non_exhaustive]
pub struct DeterminizeOptions {}

/// Cards whose location the observer knows: their own hand, everything
/// already discarded face up (played face up this round), their own face-down
/// plays, and a Farseers-peeked hand.
fn known_cards(s: &GameState, player: Player) -> [bool; ACTION_CARD_COUNT] {
    let mut known = [false; ACTION_CARD_COUNT];
    for &c in s.player(player).hand.iter() {
        known[c.as_index()] = true;
    }
    for c in s.round.played.iter() {
        if !c.face_down || c.player == player {
            known[c.card.as_index()] = true;
        }
    }
    // Cards played face up in earlier rounds of this chapter. Everyone
    // watched them go to the discard, so they cannot be in a hand — without
    // this the sampler cheerfully deals them back out, which it did in 65% of
    // worlds.
    for &c in s.revealed.iter() {
        known[c.as_index()] = true;
    }
    // A Farseers peek reveals one Rival's whole hand to the peeking player,
    // and `observe` leaves it in the view, so those cards are placed already.
    if let Some(peeked) = peeked_hand(s, player) {
        for &c in s.player(peeked).hand.iter() {
            known[c.as_index()] = true;
        }
    }
    known
}

/// Sample a full state consistent with an observation: deal the unseen action
/// cards back into the other hands, the face-down plays, the deck and the
/// discard, and reshuffle the Court deck.
///
/// The result is a legal world, not *the* world — that is exactly what
/// determinized search wants. "Legal" does real work here: cards played face
/// up earlier in the chapter are excluded, so a world can no longer contain a
/// card the observer watched being discarded.
pub fn determinize(
    obs: &Observation,
    v: &VariantDef,
    rng: &mut impl Rng,
    _opts: DeterminizeOptions,
) -> GameState {
    let mut s = obs.state;
    let known = known_cards(&s, obs.player);

    let mut pool: InlineVec<ActionCardId, ACTION_CARD_COUNT> = v
        .action_deck
        .iter()
        .copied()
        .filter(|c| !known[c.as_index()])
        .collect();
    rng.shuffle(pool.as_mut_slice());

    // Hands come off the front of the shuffled pool, face-down plays off the
    // back (the TS `splice(0, n)` / `pop()` split). Both are uniform draws;
    // the two ends only keep the two consumers from colliding.
    let peeked = peeked_hand(&s, obs.player);
    let mut front = 0usize;
    let mut back = pool.len();
    for p in (0..s.players).map(Player) {
        if p == obs.player || Some(p) == peeked {
            continue;
        }
        let want = obs.hand_sizes[p.as_index()] as usize;
        let take = want.min(back.saturating_sub(front));
        s.player_mut(p).hand = pool.as_slice()[front..front + take]
            .iter()
            .copied()
            .collect();
        front += take;
    }
    for c in s.round.played.as_mut_slice() {
        if c.card == HIDDEN_CARD {
            // A consistent observation always leaves enough pool; card 0 is
            // the TS `?? 0` fallback for one that somehow does not.
            c.card = if back > front {
                back -= 1;
                pool.as_slice()[back]
            } else {
                ActionCardId(0)
            };
        }
    }

    // Whatever is left is the deck and discard; the split does not matter
    // because a chapter deal reshuffles both together.
    s.action_deck.clear();
    s.action_discard = pool.as_slice()[front..back].iter().copied().collect();

    // The Court deck's order is unknown; rebuild it from the cards not in
    // play.
    let mut seen = [false; COURT_CARD_COUNT];
    for slot in s.court.iter() {
        seen[slot.card.as_index()] = true;
    }
    for &c in s.court_discard.iter() {
        seen[c.as_index()] = true;
    }
    for p in 0..s.players as usize {
        for &c in s.player_states[p].guild_cards.iter() {
            seen[c.as_index()] = true;
        }
    }
    let mut court_deck: InlineVec<CourtCardId, COURT_CARD_COUNT> = v
        .court_deck
        .iter()
        .copied()
        .filter(|c| !seen[c.as_index()])
        .collect();
    rng.shuffle(court_deck.as_mut_slice());
    s.court_deck = court_deck;

    s
}

/// Convenience: observe then immediately re-sample a world.
pub fn sample_world(
    s: &GameState,
    v: &VariantDef,
    player: Player,
    rng: &mut impl Rng,
) -> GameState {
    determinize(
        &observe(s, v, player),
        v,
        rng,
        DeterminizeOptions::default(),
    )
}
