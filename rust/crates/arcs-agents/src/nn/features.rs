//! `GameState` → fixed-length `[f32; FEATURE_SIZE]`, from one player's
//! perspective. Ported from `src/agents/nn/features.ts`.
//!
//! Everything is seat-rotated (index 0 is always "me", rivals follow in turn
//! order) and roughly 0-to-1 scaled, so one trained net serves every seat and
//! player count; absent seats and out-of-play systems read as zeros. The
//! layout is versioned by [`FEATURE_SIZE`] — change the layout, retrain the
//! net.
//!
//! Known gap, accepted for v1: Court and Guild cards are encoded by counts and
//! suits, not identity, so the net cannot learn card-specific tactics.
//!
//! Extraction is zero-alloc: the caller owns the buffer and the function only
//! writes into it, because the intended caller evaluates tens of thousands of
//! leaves per decision.

use arcs_engine::ambitions::{ambition_count, marker_value};
use arcs_engine::cards::{ACTION_CARD_COUNT, action_card};
use arcs_engine::court::{CourtCardKind, court_card};
use arcs_engine::map::SYSTEM_COUNT;
use arcs_engine::{
    AmbitionId, BuildingKind, GameState, MAX_SEATS, Player, ResourceType, Suit, VariantDef,
};

const SYSTEMS: usize = SYSTEM_COUNT;
const PER_SYSTEM: usize = 10;
const PER_AMBITION: usize = 6;
const PER_SEAT: usize = 12;
const HAND_EXTRAS: usize = 6;
const COURT_SLOTS: usize = 4;
const PER_COURT: usize = 4;
const GLOBALS: usize = 8;

/// Width of the feature vector, and the version of its layout. A net trained
/// against one width must be retrained if the layout moves; nothing here
/// silently reinterprets an old checkpoint.
pub const FEATURE_SIZE: usize = SYSTEMS * PER_SYSTEM
    + AmbitionId::COUNT * PER_AMBITION
    + MAX_SEATS * PER_SEAT
    + HAND_EXTRAS
    + COURT_SLOTS * PER_COURT
    + GLOBALS;

/// A caller-owned feature buffer.
pub type FeatureVec = [f32; FEATURE_SIZE];

/// The order resource slots are counted in — the TS `RESOURCE_ORDER`, which
/// is `ResourceType`'s own declaration order.
const RESOURCE_ORDER: [ResourceType; ResourceType::COUNT] = ResourceType::ALL;

/// Fill `out` with the position as seen by `player`. Zero-alloc on the hot
/// path.
pub fn extract_features(s: &GameState, v: &VariantDef, player: Player, out: &mut FeatureVec) {
    out.fill(0.0);
    let me = player.as_index();
    let players = s.players as usize;
    let mut k = 0usize;

    // --- systems ---
    for i in 0..SYSTEMS {
        let st = &s.systems[i];
        if st.out_of_play {
            k += PER_SYSTEM;
            continue;
        }
        let mut rival_max_fresh = 0u8;
        let mut rival_ships = 0u32;
        for p in 0..players {
            if p == me {
                continue;
            }
            rival_max_fresh = rival_max_fresh.max(st.fresh[p]);
            rival_ships += st.fresh[p] as u32 + st.damaged[p] as u32;
        }
        let mut my_city = 0u32;
        let mut my_starport = 0u32;
        let mut rival_cities = 0u32;
        let mut rival_starports = 0u32;
        for b in st.buildings.iter() {
            let city = b.kind() == BuildingKind::City;
            match (b.player() == player, city) {
                (true, true) => my_city += 1,
                (true, false) => my_starport += 1,
                (false, true) => rival_cities += 1,
                (false, false) => rival_starports += 1,
            }
        }
        let ctrl = s.control_of(arcs_engine::SystemId(i as u8));
        out[k] = st.fresh[me] as f32 / 4.0;
        out[k + 1] = st.damaged[me] as f32 / 4.0;
        out[k + 2] = my_city as f32 / 2.0;
        out[k + 3] = my_starport as f32 / 2.0;
        out[k + 4] = rival_max_fresh as f32 / 4.0;
        out[k + 5] = rival_ships as f32 / 8.0;
        out[k + 6] = rival_cities as f32 / 2.0;
        out[k + 7] = rival_starports as f32 / 2.0;
        out[k + 8] = if ctrl == Some(player) { 1.0 } else { 0.0 };
        out[k + 9] = if ctrl.is_some() && ctrl != Some(player) {
            1.0
        } else {
            0.0
        };
        k += PER_SYSTEM;
    }

    // --- ambitions ---
    for ambition in AmbitionId::ALL {
        let a = ambition.as_index();
        let mine = ambition_count(s.player(player), ambition);
        let mut best_rival = 0u8;
        for p in 0..players {
            if p == me {
                continue;
            }
            best_rival = best_rival.max(ambition_count(&s.player_states[p], ambition));
        }
        best_rival = best_rival.max(s.phantom[a]);
        let mut first = 0u32;
        let mut second = 0u32;
        for &i in s.declared[a].iter() {
            let val = marker_value(&v.ambition_markers, i as usize, s.flipped[i as usize]);
            first += val.first as u32;
            second += val.second as u32;
        }
        out[k] = mine as f32 / 6.0;
        out[k + 1] = best_rival as f32 / 6.0;
        out[k + 2] = first as f32 / 9.0;
        out[k + 3] = second as f32 / 6.0;
        out[k + 4] = s.declared[a].len() as f32 / 3.0;
        out[k + 5] = if mine > best_rival && mine > 0 {
            1.0
        } else {
            0.0
        };
        k += PER_AMBITION;
    }

    // --- seats, me first then rivals in turn order ---
    for seat in 0..MAX_SEATS {
        if seat >= players {
            k += PER_SEAT;
            continue;
        }
        let p = (me + seat) % players;
        let ps = &s.player_states[p];
        let open = ps.open_resource_slots();
        let mut agents = 0u32;
        for slot in s.court.iter() {
            agents += slot.agents[p] as u32;
        }
        out[k] = ps.power as f32 / v.power_threshold as f32;
        out[k + 1] = ps.hand.len() as f32 / 6.0;
        for (r, ty) in RESOURCE_ORDER.iter().enumerate() {
            let n = ps
                .resources
                .iter()
                .take(open)
                .filter(|slot| **slot == Some(*ty))
                .count();
            out[k + 2 + r] = n as f32 / 3.0;
        }
        out[k + 7] = ps.guild_cards.len() as f32 / 5.0;
        out[k + 8] = ps.captive_count() as f32 / 8.0;
        out[k + 9] = ps.trophy_count() as f32 / 8.0;
        out[k + 10] = agents as f32 / 10.0;
        out[k + 11] = if s.initiative.as_index() == p {
            1.0
        } else {
            0.0
        };
        k += PER_SEAT;
    }

    // --- my hand, beyond the size the seat block carries ---
    {
        let hand = &s.player(player).hand;
        let mut pips = 0u32;
        let mut suit_count = [0u32; Suit::COUNT];
        // deviation from features.ts: TS computes `topBySuit` by scanning
        // *every* player's hand (`for (const ps of s.playerStates)`), which
        // reads private information. That is harmless on a determinized world
        // and a leak on a raw observation, and the extractor is meant to run
        // on either. Rust reads only the observing seat's hand, so `tops`
        // counts the cards that top their suit *within my hand*. The layout
        // is unchanged; the numbers differ from TS, which is why the two
        // engines' nets are not interchangeable (FEATURE_SIZE versions the
        // layout, not the semantics — see the module docs).
        let mut top_by_suit = [0u8; Suit::COUNT];
        for &c in hand.iter() {
            let def = action_card(c);
            top_by_suit[def.suit.as_index()] = top_by_suit[def.suit.as_index()].max(def.number);
        }
        let mut tops = 0u32;
        for &c in hand.iter() {
            let def = action_card(c);
            pips += def.pips as u32;
            let si = def.suit.as_index();
            suit_count[si] += 1;
            if def.number == top_by_suit[si] {
                tops += 1;
            }
        }
        out[k] = pips as f32 / 24.0;
        out[k + 1] = tops as f32 / 4.0;
        for i in 0..Suit::COUNT {
            out[k + 2 + i] = suit_count[i] as f32 / 6.0;
        }
        k += HAND_EXTRAS;
    }

    // --- court row ---
    for slot in 0..COURT_SLOTS {
        if slot >= s.court.len() {
            k += PER_COURT;
            continue;
        }
        let c = &s.court.as_slice()[slot];
        let mut rival_max = 0u8;
        for p in 0..players {
            if p == me {
                continue;
            }
            rival_max = rival_max.max(c.agents[p]);
        }
        out[k] = c.agents[me] as f32 / 6.0;
        out[k + 1] = rival_max as f32 / 6.0;
        out[k + 2] = if court_card(c.card).kind == CourtCardKind::Vox {
            1.0
        } else {
            0.0
        };
        out[k + 3] = 1.0; // slot filled
        k += PER_COURT;
    }

    // --- globals ---
    let chapter = s.chapter.clamp(1, 5) as usize;
    out[k + chapter - 1] = 1.0; // chapter one-hot, 5 wide
    out[k + 5] = players as f32 / 4.0;
    out[k + 6] = s.action_deck.len() as f32 / ACTION_CARD_COUNT as f32;
    out[k + 7] = if s.player(player).hand.is_empty() {
        1.0
    } else {
        0.0
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feature_size_is_the_pinned_layout() {
        // Versioned contract: a net trained at 348 inputs is only valid for
        // this layout. Bumping it is a retrain, never a remap.
        assert_eq!(FEATURE_SIZE, 348);
    }
}
