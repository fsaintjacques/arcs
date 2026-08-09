//! Ambition markers and end-of-chapter scoring (rulebook p18-p19), mirroring
//! `src/engine/ambitions.ts`.

use crate::court::court_card;
use crate::inline_vec::InlineVec;
use crate::player_board::{BONUS_FIRST_BOTH_SLOTS, BONUS_FIRST_ONE_SLOT, uncovered_bonuses};
use crate::state::{MAX_SEATS, PlayerState};
use crate::types::{AmbitionId, Player, ResourceType};

/// Power for first and second place on one side of a marker.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct MarkerSide {
    pub first: u8,
    pub second: u8,
}

/// An ambition marker: Power for first and second place, per side.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct AmbitionMarkerDef {
    pub blue: MarkerSide,
    pub orange: MarkerSide,
}

pub const AMBITION_MARKER_COUNT: usize = 3;

const fn marker(bf: u8, bs: u8, of: u8, os: u8) -> AmbitionMarkerDef {
    AmbitionMarkerDef {
        blue: MarkerSide {
            first: bf,
            second: bs,
        },
        orange: MarkerSide {
            first: of,
            second: os,
        },
    }
}

/// The 3 ambition markers. Blue (starting) sides are legible in the rulebook
/// (p3): 5/3, 3/2, 2/0. See docs/DATA-GAPS.md §1 for the orange-side
/// sourcing.
pub const AMBITION_MARKERS: [AmbitionMarkerDef; AMBITION_MARKER_COUNT] =
    [marker(5, 3, 9, 5), marker(3, 2, 6, 4), marker(2, 0, 4, 2)];

/// "Take the highest-number ambition marker from the Available Markers
/// section" (p9) — highest by current first-place Power.
/// (`highestAvailable` in ambitions.ts.)
pub fn highest_available(s: &crate::state::GameState, v: &crate::setup::VariantDef) -> Option<u8> {
    let mut best = None;
    let mut best_value = 0i16;
    for &i in s.available_markers.iter() {
        let value =
            marker_value(&v.ambition_markers, i as usize, s.flipped[i as usize]).first as i16;
        if value > best_value {
            best_value = value;
            best = Some(i);
        }
    }
    best
}

/// "Flip over the ambition marker with the lowest Power that hasn't been
/// flipped yet to its side with more Power" (p19).
/// (`flipLowestUnflipped` in ambitions.ts.)
pub fn flip_lowest_unflipped(s: &mut crate::state::GameState, v: &crate::setup::VariantDef) {
    let mut target = None;
    let mut lowest = u8::MAX;
    for i in 0..v.ambition_markers.len() {
        if s.flipped[i] {
            continue;
        }
        let value = v.ambition_markers[i].blue.first;
        if value < lowest {
            lowest = value;
            target = Some(i);
        }
    }
    if let Some(i) = target {
        s.flipped[i] = true;
    }
}

/// Power a marker is currently worth, given whether it has been flipped.
#[inline]
pub const fn marker_value(
    markers: &[AmbitionMarkerDef; AMBITION_MARKER_COUNT],
    index: usize,
    flipped: bool,
) -> MarkerSide {
    let m = &markers[index];
    if flipped { m.orange } else { m.blue }
}

// ---------------------------------------------------------------------------
// Counting ambitions
// ---------------------------------------------------------------------------

/// Resource + Guild-card icons of the given types a player holds. `cartel`
/// adds the resources sitting on a Cartel card this player holds: "you add
/// it to Tycoon but can't spend it" (p20). (`icons` in ambitions.ts.)
fn icons(p: &PlayerState, cartel: &crate::types::ByResource<u8>, types: &[ResourceType]) -> u8 {
    let mut n = 0;
    for r in p.resources.iter().flatten() {
        if types.contains(r) {
            n += 1;
        }
    }
    for &id in p.guild_cards.iter() {
        if let Some(suit) = court_card(id).suit
            && types.contains(&suit)
        {
            n += 1;
        }
    }
    for &t in types {
        n += cartel[t.as_index()];
    }
    n
}

/// What a player has toward one ambition (p18), including Cartel supplies.
/// (`ambitionCount` with the `cartel` argument in ambitions.ts.)
pub fn ambition_count_with(
    p: &PlayerState,
    ambition: AmbitionId,
    cartel: &crate::types::ByResource<u8>,
) -> u8 {
    use ResourceType::{Fuel, Material, Psionic, Relic};
    match ambition {
        AmbitionId::Tycoon => icons(p, cartel, &[Material, Fuel]),
        AmbitionId::Tyrant => p.captive_count(),
        AmbitionId::Warlord => p.trophy_count(),
        AmbitionId::Keeper => icons(p, cartel, &[Relic]),
        AmbitionId::Empath => icons(p, cartel, &[Psionic]),
    }
}

/// [`ambition_count_with`] without Cartel supplies (the TS call shape with
/// the optional `cartel` argument omitted).
pub fn ambition_count(p: &PlayerState, ambition: AmbitionId) -> u8 {
    ambition_count_with(p, ambition, &[0; ResourceType::COUNT])
}

/// One scored ambition box. Seats beyond `players` stay zero.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct AmbitionResult {
    pub ambition: AmbitionId,
    /// Summed first-place Power of every marker in the box.
    pub first: u8,
    /// Summed second-place Power.
    pub second: u8,
    /// Counts per player, plus the 2-player phantom alongside.
    pub counts: [u8; MAX_SEATS],
    pub phantom: u8,
    pub first_place: InlineVec<Player, MAX_SEATS>,
    pub second_place: InlineVec<Player, MAX_SEATS>,
    pub awards: [u8; MAX_SEATS],
}

/// Score one ambition box (`scoreAmbition` in ambitions.ts).
///
/// Ties for first: all tied players take second place instead. Ties for
/// second: nobody places. You cannot place with a count of 0 ("Qualifying",
/// p18). Bonus city Power applies only to an outright first place.
pub fn score_ambition(
    s: &crate::state::GameState,
    v: &crate::setup::VariantDef,
    ambition: AmbitionId,
) -> Option<AmbitionResult> {
    let markers = &s.declared[ambition.as_index()];
    if markers.is_empty() {
        return None;
    }

    let mut first = 0u8;
    let mut second = 0u8;
    for &i in markers.iter() {
        let value = marker_value(&v.ambition_markers, i as usize, s.flipped[i as usize]);
        first += value.first;
        second += value.second;
    }

    let players = s.players as usize;
    let mut counts = [0u8; MAX_SEATS];
    for (i, c) in counts.iter_mut().enumerate().take(players) {
        let cartel = crate::powers::cartel_icons(s, Player(i as u8));
        *c = ambition_count_with(&s.player_states[i], ambition, &cartel);
    }
    let phantom = s.phantom[ambition.as_index()];

    let top = counts[..players]
        .iter()
        .copied()
        .max()
        .unwrap()
        .max(phantom);
    let mut awards = [0u8; MAX_SEATS];
    let mut first_place = InlineVec::new();
    let mut second_place = InlineVec::new();

    if top > 0 {
        let top_players: InlineVec<usize, MAX_SEATS> = (0..players)
            .filter(|&i| counts[i] == top)
            .collect::<InlineVec<usize, MAX_SEATS>>();
        let phantom_top = phantom == top;
        let tied_for_first = top_players.len() + usize::from(phantom_top) > 1;

        if !tied_for_first && top_players.len() == 1 {
            // Outright first place.
            let winner = top_players.as_slice()[0];
            first_place.push(Player(winner as u8));
            awards[winner] += first + bonus_city_power(&s.player_states[winner]);

            // Second place goes to the next distinct positive count, unless
            // tied.
            let runner_up = counts[..players]
                .iter()
                .copied()
                .chain(core::iter::once(phantom))
                .filter(|&c| c < top && c > 0)
                .max();
            if let Some(runner_up) = runner_up {
                let runners: InlineVec<usize, MAX_SEATS> =
                    (0..players).filter(|&i| counts[i] == runner_up).collect();
                let phantom_runner = phantom == runner_up;
                if runners.len() + usize::from(phantom_runner) == 1 && runners.len() == 1 {
                    let r = runners.as_slice()[0];
                    second_place.push(Player(r as u8));
                    awards[r] += second;
                }
            }
        } else {
            // Everyone tied for first drops to second place; no bonus city
            // Power.
            for &i in top_players.iter() {
                second_place.push(Player(i as u8));
                awards[i] += second;
            }
        }
    }

    Some(AmbitionResult {
        ambition,
        first,
        second,
        counts,
        phantom,
        first_place,
        second_place,
        awards,
    })
}

/// +2 / +5 for an outright first place, by uncovered city slots (p18).
/// (`bonusCityPower` in ambitions.ts.)
pub fn bonus_city_power(p: &PlayerState) -> u8 {
    let (plus_two, plus_three) = uncovered_bonuses(p.cities_used);
    if plus_two && plus_three {
        BONUS_FIRST_BOTH_SLOTS
    } else if plus_two {
        BONUS_FIRST_ONE_SLOT
    } else {
        0
    }
}

/// Score every declared ambition box (`scoreChapter` in ambitions.ts).
pub struct ChapterScore {
    pub results: InlineVec<AmbitionResult, { AmbitionId::COUNT }>,
    pub gained: [u8; MAX_SEATS],
}

pub fn score_chapter(s: &crate::state::GameState, v: &crate::setup::VariantDef) -> ChapterScore {
    let mut results = InlineVec::filled(AmbitionResult {
        ambition: AmbitionId::Tycoon,
        first: 0,
        second: 0,
        counts: [0; MAX_SEATS],
        phantom: 0,
        first_place: InlineVec::new(),
        second_place: InlineVec::new(),
        awards: [0; MAX_SEATS],
    });
    let mut gained = [0u8; MAX_SEATS];
    for ambition in AmbitionId::ALL {
        let Some(r) = score_ambition(s, v, ambition) else {
            continue;
        };
        for (g, a) in gained.iter_mut().zip(r.awards) {
            *g += a;
        }
        results.push(r);
    }
    ChapterScore { results, gained }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marker_values_flip() {
        let side = marker_value(&AMBITION_MARKERS, 0, false);
        assert_eq!((side.first, side.second), (5, 3));
        let side = marker_value(&AMBITION_MARKERS, 0, true);
        assert_eq!((side.first, side.second), (9, 5));
        let side = marker_value(&AMBITION_MARKERS, 2, true);
        assert_eq!((side.first, side.second), (4, 2));
    }
}
