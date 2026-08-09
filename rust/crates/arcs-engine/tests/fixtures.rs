//! Const-data parity tests against fixtures printed from the TS engine
//! (the source of truth). Regenerate with the `npx tsx` one-liners in the
//! R0 PR description; the fixtures are checked in so CI needs no Node.

use arcs_engine::ambitions::AMBITION_MARKERS;
use arcs_engine::cards::{ALL_ACTION_CARDS, CardAmbition, action_deck_for};
use arcs_engine::court::{COURT_DECK, CourtCardKind, court_row_size};
use arcs_engine::dice::{DICE_PER_TYPE, DIE_FACES, expected_face};
use arcs_engine::map::{SystemKind, build_systems, resolve_adjacency};
use arcs_engine::player_board::{
    AGENTS, BASE_RESOURCE_SLOTS, BONUS_FIRST_BOTH_SLOTS, BONUS_FIRST_ONE_SLOT, CITY_SLOT_REWARDS,
    CITY_SLOTS, CityReward, MAX_RESOURCE_SLOTS, RAID_COSTS, SHIPS, STARPORTS,
};
use arcs_engine::setup::{HAND_SIZE, MAX_CHAPTERS, power_threshold, setup_deck};
use arcs_engine::types::{AmbitionId, DieType, ResourceType, Suit};
use serde_json::Value;

fn fixture(name: &str) -> Value {
    let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    let data = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path}: {e}"));
    serde_json::from_str(&data).unwrap()
}

fn as_u8(v: &Value) -> u8 {
    v.as_u64().unwrap() as u8
}

fn resource(v: &Value) -> Option<ResourceType> {
    match v.as_str()? {
        "material" => Some(ResourceType::Material),
        "fuel" => Some(ResourceType::Fuel),
        "weapon" => Some(ResourceType::Weapon),
        "relic" => Some(ResourceType::Relic),
        "psionic" => Some(ResourceType::Psionic),
        other => panic!("unknown resource {other}"),
    }
}

fn suit(v: &Value) -> Suit {
    match v.as_str().unwrap() {
        "administration" => Suit::Administration,
        "aggression" => Suit::Aggression,
        "construction" => Suit::Construction,
        "mobilization" => Suit::Mobilization,
        other => panic!("unknown suit {other}"),
    }
}

fn sorted_ids(v: &Value) -> Vec<u8> {
    let mut ids: Vec<u8> = v.as_array().unwrap().iter().map(as_u8).collect();
    ids.sort_unstable();
    ids
}

#[test]
fn systems_match_ts() {
    let fixture = fixture("systems.json");
    let systems = build_systems();
    let expected = fixture.as_array().unwrap();
    assert_eq!(expected.len(), systems.len());
    for (s, e) in systems.iter().zip(expected) {
        assert_eq!(s.id.0, as_u8(&e["id"]));
        assert_eq!(s.cluster, as_u8(&e["cluster"]));
        assert_eq!(s.slot, as_u8(&e["slot"]));
        let kind = match e["kind"].as_str().unwrap() {
            "gate" => SystemKind::Gate,
            _ => SystemKind::Planet,
        };
        assert_eq!(s.kind, kind);
        assert_eq!(s.planet_type, resource(&e["planetType"]));
        assert_eq!(s.building_slots, as_u8(&e["buildingSlots"]));
        assert_eq!(s.label(), e["label"].as_str().unwrap());
        let mut ours: Vec<u8> = s.adjacent.iter().map(|a| a.0).collect();
        ours.sort_unstable();
        assert_eq!(ours, sorted_ids(&e["adjacent"]), "system {}", s.id.0);
    }
}

#[test]
fn resolved_adjacency_matches_ts() {
    let fixture = fixture("adjacency.json");
    let base = build_systems();
    for (key, expected) in fixture.as_object().unwrap() {
        let out_of_play: Vec<u8> = key.split('_').map(|c| c.parse().unwrap()).collect();
        let resolved = resolve_adjacency(&base, &out_of_play);
        for (s, e) in resolved.iter().zip(expected.as_array().unwrap()) {
            assert_eq!(s.id.0, as_u8(&e["id"]));
            let mut ours: Vec<u8> = s.adjacent.iter().map(|a| a.0).collect();
            ours.sort_unstable();
            assert_eq!(
                ours,
                sorted_ids(&e["adjacent"]),
                "out {key} system {}",
                s.id.0
            );
        }
    }
}

#[test]
fn action_cards_match_ts() {
    let fixture = fixture("action_cards.json");
    let all = fixture["all"].as_array().unwrap();
    assert_eq!(all.len(), ALL_ACTION_CARDS.len());
    for (c, e) in ALL_ACTION_CARDS.iter().zip(all) {
        assert_eq!(c.id.0, as_u8(&e["id"]));
        assert_eq!(c.suit, suit(&e["suit"]));
        assert_eq!(c.number, as_u8(&e["number"]));
        assert_eq!(c.pips, as_u8(&e["pips"]));
        let ambition = match &e["ambition"] {
            Value::Null => CardAmbition::None,
            Value::String(s) if s == "any" => CardAmbition::Any,
            Value::String(s) => CardAmbition::Some(match s.as_str() {
                "tycoon" => AmbitionId::Tycoon,
                "tyrant" => AmbitionId::Tyrant,
                "warlord" => AmbitionId::Warlord,
                "keeper" => AmbitionId::Keeper,
                "empath" => AmbitionId::Empath,
                other => panic!("unknown ambition {other}"),
            }),
            other => panic!("bad ambition {other}"),
        };
        assert_eq!(c.ambition(), ambition, "card {}", c.id.0);
    }
    for (players, key) in [(2u8, "deck2"), (3, "deck3"), (4, "deck4")] {
        let ours: Vec<u8> = action_deck_for(players).iter().map(|id| id.0).collect();
        let theirs: Vec<u8> = fixture[key].as_array().unwrap().iter().map(as_u8).collect();
        assert_eq!(ours, theirs, "{players}p deck");
    }
}

#[test]
fn court_deck_matches_ts() {
    let fixture = fixture("court.json");
    let expected = fixture.as_array().unwrap();
    assert_eq!(expected.len(), COURT_DECK.len());
    for (c, e) in COURT_DECK.iter().zip(expected) {
        assert_eq!(c.id.0, as_u8(&e["id"]), "{}", c.name);
        assert_eq!(c.number, as_u8(&e["number"]), "{}", c.name);
        assert_eq!(c.name, e["name"].as_str().unwrap());
        let kind = match e["kind"].as_str().unwrap() {
            "guild" => CourtCardKind::Guild,
            _ => CourtCardKind::Vox,
        };
        assert_eq!(c.kind, kind, "{}", c.name);
        assert_eq!(c.suit, resource(&e["suit"]), "{}", c.name);
        assert_eq!(c.raid_cost, as_u8(&e["raidCost"]), "{}", c.name);
    }
}

#[test]
fn die_faces_match_ts() {
    let fixture = fixture("dice.json");
    assert_eq!(
        fixture["dicePerType"].as_u64().unwrap() as usize,
        DICE_PER_TYPE
    );
    for (die, key) in [
        (DieType::Assault, "assault"),
        (DieType::Skirmish, "skirmish"),
        (DieType::Raid, "raid"),
    ] {
        let faces = fixture["faces"][key].as_array().unwrap();
        assert_eq!(faces.len(), DIE_FACES[die.as_index()].len());
        for (f, e) in DIE_FACES[die.as_index()].iter().zip(faces) {
            assert_eq!(f.hits, as_u8(&e["hits"]), "{key}");
            assert_eq!(f.self_hits, as_u8(&e["selfHits"]), "{key}");
            assert_eq!(f.building_hits, as_u8(&e["buildingHits"]), "{key}");
            assert_eq!(f.keys, as_u8(&e["keys"]), "{key}");
            assert_eq!(f.intercept, as_u8(&e["intercept"]), "{key}");
        }
        let expect = &fixture["expected"][key];
        let ours = expected_face(die);
        for (field, value) in [
            ("hits", ours.hits),
            ("selfHits", ours.self_hits),
            ("buildingHits", ours.building_hits),
            ("keys", ours.keys),
            ("intercept", ours.intercept),
        ] {
            let theirs = expect[field].as_f64().unwrap();
            assert!((value - theirs).abs() < 1e-12, "{key} {field}");
        }
    }
}

#[test]
fn ambition_markers_match_ts() {
    let fixture = fixture("ambition_markers.json");
    let expected = fixture.as_array().unwrap();
    assert_eq!(expected.len(), AMBITION_MARKERS.len());
    for (m, e) in AMBITION_MARKERS.iter().zip(expected) {
        assert_eq!(m.blue.first, as_u8(&e["blue"]["first"]));
        assert_eq!(m.blue.second, as_u8(&e["blue"]["second"]));
        assert_eq!(m.orange.first, as_u8(&e["orange"]["first"]));
        assert_eq!(m.orange.second, as_u8(&e["orange"]["second"]));
    }
}

#[test]
fn setup_deck_matches_ts() {
    let fixture = fixture("setup_deck.json");
    for players in 2..=4u8 {
        let cards = setup_deck(players);
        let expected = fixture[players.to_string()].as_array().unwrap();
        assert_eq!(expected.len(), cards.len());
        for (c, e) in cards.iter().zip(expected) {
            assert_eq!(c.name, e["name"].as_str().unwrap());
            let out: Vec<u8> = c.out_of_play.iter().copied().collect();
            assert_eq!(
                out,
                e["outOfPlay"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(as_u8)
                    .collect::<Vec<u8>>()
            );
            let starts = e["starts"].as_array().unwrap();
            assert_eq!(c.starts.len(), starts.len());
            for (s, se) in c.starts.iter().zip(starts) {
                assert_eq!(s.a.0, as_u8(&se["a"]), "{players}p {}", c.name);
                assert_eq!(s.b.0, as_u8(&se["b"]), "{players}p {}", c.name);
                let cs: Vec<u8> = s.c.iter().map(|x| x.0).collect();
                assert_eq!(
                    cs,
                    se["c"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(as_u8)
                        .collect::<Vec<u8>>()
                );
            }
        }
    }
}

#[test]
fn constants_match_ts() {
    let fixture = fixture("constants.json");
    for players in 2..=4u8 {
        let key = players.to_string();
        assert_eq!(
            power_threshold(players),
            as_u8(&fixture["powerThreshold"][&key])
        );
        assert_eq!(
            court_row_size(players),
            as_u8(&fixture["courtRowSize"][&key])
        );
    }
    assert_eq!(MAX_CHAPTERS, as_u8(&fixture["maxChapters"]));
    assert_eq!(HAND_SIZE, as_u8(&fixture["handSize"]));

    let board = &fixture["playerBoard"];
    assert_eq!(
        BASE_RESOURCE_SLOTS,
        board["baseResourceSlots"].as_u64().unwrap() as usize
    );
    assert_eq!(
        MAX_RESOURCE_SLOTS,
        board["maxResourceSlots"].as_u64().unwrap() as usize
    );
    assert_eq!(
        CITY_SLOTS,
        board["citySlotCount"].as_u64().unwrap() as usize
    );
    assert_eq!(SHIPS, as_u8(&board["ships"]));
    assert_eq!(STARPORTS, as_u8(&board["starports"]));
    assert_eq!(AGENTS, as_u8(&board["agents"]));
    assert_eq!(BONUS_FIRST_ONE_SLOT, as_u8(&board["bonusFirstOneSlot"]));
    assert_eq!(BONUS_FIRST_BOTH_SLOTS, as_u8(&board["bonusFirstBothSlots"]));
    let raid_costs: Vec<u8> = board["raidCosts"]
        .as_array()
        .unwrap()
        .iter()
        .map(as_u8)
        .collect();
    assert_eq!(RAID_COSTS.to_vec(), raid_costs);
    let rewards: Vec<CityReward> = board["citySlots"]
        .as_array()
        .unwrap()
        .iter()
        .map(|slot| match slot["kind"].as_str().unwrap() {
            "resource" => CityReward::Resource,
            "plusTwo" => CityReward::PlusTwo,
            "plusThree" => CityReward::PlusThree,
            other => panic!("unknown reward {other}"),
        })
        .collect();
    assert_eq!(CITY_SLOT_REWARDS.to_vec(), rewards);
}
