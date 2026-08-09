//! The player board: resource slots, city slots and the rewards they uncover
//! (rulebook p7, p17, p18), mirroring `src/engine/playerBoard.ts`.
//!
//! Cities sit in slots along the top of the board and are placed
//! leftmost-first; as they leave the board they uncover resource slots and
//! the two "to won ambitions" Power bonuses.

pub const CITY_SLOTS: usize = 5;
pub const SHIPS: u8 = 15;
pub const STARPORTS: u8 = 5;
pub const AGENTS: u8 = 10;

/// Bonus Power for an outright ambition win, by uncovered slots (p18).
pub const BONUS_FIRST_ONE_SLOT: u8 = 2;
pub const BONUS_FIRST_BOTH_SLOTS: u8 = 5;

/// What each city slot uncovers when its city is built.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum CityReward {
    Resource,
    PlusTwo,
    PlusThree,
}

/// The city-slot rewards, leftmost first.
pub const CITY_SLOT_REWARDS: [CityReward; CITY_SLOTS] = [
    CityReward::Resource,
    CityReward::Resource,
    CityReward::PlusTwo,
    CityReward::Resource,
    CityReward::PlusThree,
];

/// Slots open before any city is built (p5 step O gives 2 starting
/// resources).
pub const BASE_RESOURCE_SLOTS: usize = 3;

/// Keys an attacker must spend to steal the resource in each slot (p16-p17).
pub const RAID_COSTS: [u8; 6] = [1, 1, 2, 2, 3, 3];

/// Total resource slots once every city is built.
pub const MAX_RESOURCE_SLOTS: usize = {
    let mut n = BASE_RESOURCE_SLOTS;
    let mut i = 0;
    while i < CITY_SLOTS {
        if matches!(CITY_SLOT_REWARDS[i], CityReward::Resource) {
            n += 1;
        }
        i += 1;
    }
    n
};

/// How many resource slots are open for a player who has moved `cities_used`
/// cities off the board.
pub const fn open_resource_slots(cities_used: u8) -> usize {
    let mut n = BASE_RESOURCE_SLOTS;
    let mut i = 0;
    while i < cities_used as usize && i < CITY_SLOTS {
        if matches!(CITY_SLOT_REWARDS[i], CityReward::Resource) {
            n += 1;
        }
        i += 1;
    }
    if n > MAX_RESOURCE_SLOTS {
        MAX_RESOURCE_SLOTS
    } else {
        n
    }
}

/// The uncovered "to won ambitions" bonuses for `cities_used`.
pub const fn uncovered_bonuses(cities_used: u8) -> (bool, bool) {
    let mut plus_two = false;
    let mut plus_three = false;
    let mut i = 0;
    while i < cities_used as usize && i < CITY_SLOTS {
        match CITY_SLOT_REWARDS[i] {
            CityReward::PlusTwo => plus_two = true,
            CityReward::PlusThree => plus_three = true,
            CityReward::Resource => {}
        }
        i += 1;
    }
    (plus_two, plus_three)
}

/// Raid cost of a held resource, by slot index.
#[inline]
pub const fn raid_cost(slot: usize) -> u8 {
    let i = if slot < RAID_COSTS.len() {
        slot
    } else {
        RAID_COSTS.len() - 1
    };
    RAID_COSTS[i]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn max_resource_slots_is_six() {
        assert_eq!(MAX_RESOURCE_SLOTS, 6);
    }

    #[test]
    fn slots_open_as_cities_leave() {
        assert_eq!(open_resource_slots(0), 3);
        assert_eq!(open_resource_slots(1), 4);
        assert_eq!(open_resource_slots(2), 5);
        assert_eq!(open_resource_slots(3), 5); // slot 3 is the +2 bonus
        assert_eq!(open_resource_slots(4), 6);
        assert_eq!(open_resource_slots(5), 6); // slot 5 is the +3 bonus
        assert_eq!(open_resource_slots(9), 6);
    }

    #[test]
    fn bonuses_uncover_in_order() {
        assert_eq!(uncovered_bonuses(2), (false, false));
        assert_eq!(uncovered_bonuses(3), (true, false));
        assert_eq!(uncovered_bonuses(5), (true, true));
    }

    #[test]
    fn raid_costs_clamp() {
        assert_eq!(raid_cost(0), 1);
        assert_eq!(raid_cost(4), 3);
        assert_eq!(raid_cost(99), 3);
    }
}
