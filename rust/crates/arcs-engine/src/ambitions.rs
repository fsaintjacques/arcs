//! Ambition markers (rulebook p18-p19), mirroring `src/engine/ambitions.ts`.

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
