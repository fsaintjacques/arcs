//! TypeScript-compatible JSON for the browser UI.
//!
//! The React app in `src/ui/` was written against the TS engine's
//! `src/engine/types.ts`: camelCase fields, `Record<ResourceType, T>` objects,
//! `trophies: {owner, kind}[]`, `Action` as a `{ t: '...' }` tagged union.
//! The Rust state is a flat POD — count matrices, packed buildings, `[T; 5]`
//! maps, snake_case — so this module is the translation layer that lets
//! `Board.tsx`, `Panels.tsx`, `describe.ts` and friends keep working
//! unchanged against the Rust engine.
//!
//! It is a **projection, not a codec**: nothing here parses TS JSON back into
//! Rust. The UI chooses actions by index into the legal list (see
//! [`crate::Session`]), which is the contract every FFI binding crosses, so
//! the TS-shaped `Action` values are display data only.
//!
//! Known lossy spots, all of them invisible to the UI and documented here
//! rather than faked:
//!
//! - **Trophy order.** The Rust state counts trophies per (owner, kind); the
//!   emitted list is rebuilt in owner-then-kind order. Nothing renders order
//!   (`Panels.tsx` shows `trophies.length`).
//! - **Prelude spend order.** `TurnState::prelude_spent` is a per-type tally
//!   in Rust, so the emitted list is grouped by type. Nothing reads it.
//! - **Printed Court card text.** The Rust port carries card *rules*, not
//!   card *prose*, so `CourtCardDef.text` comes out empty; the UI fills it
//!   from its own static table, which is where the printed text has always
//!   lived (`src/ui/session.ts`).

use arcs_engine::action::{CardList, ResourceList};
use arcs_engine::ambitions::marker_value;
use arcs_engine::cards::{ActionCardDef, CardAmbition, action_card};
use arcs_engine::court::{CourtCardKind, court_card};
use arcs_engine::inline_vec::InlineVec;
use arcs_engine::map::{SYSTEM_COUNT, SystemKind};
use arcs_engine::state::{
    ActionKindSet, BattleState, Building, GameState, MoveState, PlayedCard, PlayerState,
    TrophyKind, VoxResume,
};
use arcs_engine::{
    Action, ActionKind, AmbitionId, BuildingKind, CardActionName, CourtCardId, DieType, FollowMode,
    HitTarget, Phase, PlayMode, Player, ResourceType, Standing, Suit, SystemId, VariantDef,
};
use serde::Serialize;

// ---------------------------------------------------------------------------
// Vocabulary: the TS string unions
// ---------------------------------------------------------------------------

const RESOURCE_NAMES: [&str; 5] = ["material", "fuel", "weapon", "relic", "psionic"];
const SUIT_NAMES: [&str; 4] = [
    "administration",
    "aggression",
    "construction",
    "mobilization",
];
const ACTION_KIND_NAMES: [&str; 7] = [
    "tax",
    "build",
    "move",
    "repair",
    "influence",
    "secure",
    "battle",
];
const AMBITION_NAMES: [&str; 5] = ["tycoon", "tyrant", "warlord", "keeper", "empath"];
const BUILDING_KIND_NAMES: [&str; 2] = ["city", "starport"];
const PLAY_MODE_NAMES: [&str; 4] = ["lead", "surpass", "copy", "pivot"];
const TROPHY_KIND_NAMES: [&str; 4] = ["ship", "starport", "city", "agent"];
/// Indexed by [`Phase::as_index`]; `LeaderDraft` has no TS counterpart yet.
const PHASE_NAMES: [&str; 14] = [
    "leaderDraft",
    "deal",
    "mulligan",
    "play",
    "prelude",
    "actions",
    "catapult",
    "battleRoll",
    "battleReroll",
    "battleAssign",
    "peekTarget",
    "peekSwap",
    "reinforce",
    "over",
];

fn resource(r: ResourceType) -> &'static str {
    RESOURCE_NAMES[r.as_index()]
}
fn suit(s: Suit) -> &'static str {
    SUIT_NAMES[s.as_index()]
}
fn action_kind(k: ActionKind) -> &'static str {
    ACTION_KIND_NAMES[k.as_index()]
}
fn ambition(a: AmbitionId) -> &'static str {
    AMBITION_NAMES[a.as_index()]
}
fn building_kind(k: BuildingKind) -> &'static str {
    BUILDING_KIND_NAMES[k.as_index()]
}
fn play_mode(m: PlayMode) -> &'static str {
    PLAY_MODE_NAMES[m.as_index()]
}
fn phase(p: Phase) -> &'static str {
    PHASE_NAMES[p.as_index()]
}

/// The TS `Record<ResourceType, T>` object, written out longhand so the key
/// order and spelling are the interface rather than an incidental map order.
#[derive(Serialize)]
pub struct ByResourceJson<T> {
    pub material: T,
    pub fuel: T,
    pub weapon: T,
    pub relic: T,
    pub psionic: T,
}

impl<T: Copy> ByResourceJson<T> {
    fn from_array(a: &[T; 5]) -> Self {
        ByResourceJson {
            material: a[0],
            fuel: a[1],
            weapon: a[2],
            relic: a[3],
            psionic: a[4],
        }
    }
}

/// The TS `Record<AmbitionId, T>` object.
#[derive(Serialize)]
pub struct ByAmbitionJson<T> {
    pub tycoon: T,
    pub tyrant: T,
    pub warlord: T,
    pub keeper: T,
    pub empath: T,
}

impl<T> ByAmbitionJson<T> {
    fn from_fn(mut f: impl FnMut(AmbitionId) -> T) -> Self {
        ByAmbitionJson {
            tycoon: f(AmbitionId::Tycoon),
            tyrant: f(AmbitionId::Tyrant),
            warlord: f(AmbitionId::Warlord),
            keeper: f(AmbitionId::Keeper),
            empath: f(AmbitionId::Empath),
        }
    }
}

/// The TS `Record<DieType, T>` object.
#[derive(Serialize)]
pub struct ByDieJson<T> {
    pub assault: T,
    pub skirmish: T,
    pub raid: T,
}

impl<T> ByDieJson<T> {
    fn from_fn(mut f: impl FnMut(DieType) -> T) -> Self {
        ByDieJson {
            assault: f(DieType::Assault),
            skirmish: f(DieType::Skirmish),
            raid: f(DieType::Raid),
        }
    }
}

// ---------------------------------------------------------------------------
// GameState
// ---------------------------------------------------------------------------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildingJson {
    pub player: u8,
    pub kind: &'static str,
    pub damaged: bool,
    pub taxed_this_turn: bool,
    pub built_this_turn: bool,
}

impl From<Building> for BuildingJson {
    fn from(b: Building) -> Self {
        BuildingJson {
            player: b.player().0,
            kind: building_kind(b.kind()),
            damaged: b.damaged(),
            taxed_this_turn: b.taxed_this_turn(),
            built_this_turn: b.built_this_turn(),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemStateJson {
    /// Per-player, truncated to the seats in play (the TS arrays are
    /// `players` long; the Rust ones are always `MAX_SEATS`).
    pub fresh: Vec<u8>,
    pub damaged: Vec<u8>,
    pub buildings: Vec<BuildingJson>,
    pub out_of_play: bool,
}

#[derive(Serialize)]
pub struct TrophyJson {
    pub owner: u8,
    pub kind: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerStateJson {
    pub power: u8,
    pub resources: Vec<Option<&'static str>>,
    pub outrage: ByResourceJson<bool>,
    pub guild_cards: Vec<u8>,
    pub trophies: Vec<TrophyJson>,
    pub captives: Vec<u8>,
    pub agents_supply: u8,
    pub ships_supply: u8,
    pub starports_supply: u8,
    pub cities_used: u8,
    pub hand: Vec<u8>,
}

fn player_state_json(p: &PlayerState, players: u8) -> PlayerStateJson {
    // Rebuild the TS lists from the count matrices. Capture order is gone;
    // see the module docs for why nothing misses it.
    let mut trophies = Vec::new();
    for (owner, kinds) in p.trophies.iter().enumerate().take(players as usize) {
        for (kind, &n) in kinds.iter().enumerate() {
            for _ in 0..n {
                trophies.push(TrophyJson {
                    owner: owner as u8,
                    kind: TROPHY_KIND_NAMES[kind],
                });
            }
        }
    }
    let mut captives = Vec::new();
    for (owner, &n) in p.captives.iter().enumerate().take(players as usize) {
        for _ in 0..n {
            captives.push(owner as u8);
        }
    }
    PlayerStateJson {
        power: p.power,
        resources: p.resources.iter().map(|r| r.map(resource)).collect(),
        outrage: ByResourceJson::from_array(&p.outrage),
        guild_cards: p.guild_cards.iter().map(|c| c.0).collect(),
        trophies,
        captives,
        agents_supply: p.agents_supply,
        ships_supply: p.ships_supply,
        starports_supply: p.starports_supply,
        cities_used: p.cities_used,
        hand: p.hand.iter().map(|c| c.0).collect(),
    }
}

#[derive(Serialize)]
pub struct CourtSlotJson {
    pub card: u8,
    pub agents: Vec<u8>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayedCardJson {
    pub player: u8,
    pub card: u8,
    pub mode: &'static str,
    pub face_down: bool,
}

impl From<PlayedCard> for PlayedCardJson {
    fn from(c: PlayedCard) -> Self {
        PlayedCardJson {
            player: c.player.0,
            card: c.card.0,
            mode: play_mode(c.mode),
            face_down: c.face_down,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoundStateJson {
    pub turn_index: u8,
    pub turn_order: Vec<u8>,
    pub lead: Option<PlayedCardJson>,
    pub lead_number: u8,
    pub played: Vec<PlayedCardJson>,
    pub seized_by: Option<u8>,
    pub consecutive_passes: u8,
    pub ambition_declared: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TurnStateJson {
    pub player: u8,
    pub mode: &'static str,
    pub card: u8,
    pub pips_left: u8,
    pub pip_actions: Vec<&'static str>,
    pub free_actions: Vec<Vec<&'static str>>,
    pub weapon_spent: bool,
    pub prelude_over: bool,
    pub declared_this_turn: bool,
    pub prelude_spent: Vec<&'static str>,
    pub secured_this_prelude: Vec<u8>,
    pub card_preludes_used: Vec<u8>,
}

fn kinds(set: ActionKindSet) -> Vec<&'static str> {
    set.iter().map(action_kind).collect()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BattleStateJson {
    pub rolled: ByDieJson<Vec<u8>>,
    pub system: u8,
    pub attacker: u8,
    pub defender: u8,
    pub dice: ByDieJson<u8>,
    pub self_hits: u8,
    pub intercept: u8,
    pub hits: u8,
    pub building_hits: u8,
    pub keys: u8,
    pub intercept_resolved: bool,
    pub skirmish_blanks: u8,
    pub pending_reroll: u8,
    pub reroll_done: bool,
}

impl From<BattleState> for BattleStateJson {
    fn from(b: BattleState) -> Self {
        BattleStateJson {
            rolled: ByDieJson::from_fn(|d| b.rolled[d.as_index()].iter().copied().collect()),
            system: b.system.0,
            attacker: b.attacker.0,
            defender: b.defender.0,
            dice: ByDieJson::from_fn(|d| b.dice[d.as_index()]),
            self_hits: b.self_hits,
            intercept: b.intercept,
            hits: b.hits,
            building_hits: b.building_hits,
            keys: b.keys,
            intercept_resolved: b.intercept_resolved,
            skirmish_blanks: b.skirmish_blanks,
            pending_reroll: b.pending_reroll,
            reroll_done: b.reroll_done,
        }
    }
}

#[derive(Serialize)]
pub struct MoveStateJson {
    pub at: u8,
    pub ships: u8,
    pub visited: Vec<u8>,
}

impl From<MoveState> for MoveStateJson {
    fn from(m: MoveState) -> Self {
        MoveStateJson {
            at: m.at.0,
            ships: m.ships,
            visited: m.visited.iter().map(|s| s.0).collect(),
        }
    }
}

#[derive(Serialize)]
pub struct UnionJson {
    pub card: u8,
    pub player: u8,
    pub target: u8,
}

#[derive(Serialize)]
pub struct PendingVoxJson {
    pub card: u8,
    pub player: u8,
    pub resume: &'static str,
}

#[derive(Serialize)]
pub struct PeekJson {
    pub player: u8,
    pub target: Option<u8>,
    pub resume: &'static str,
}

#[derive(Serialize)]
pub struct DeclineJson {
    pub player: u8,
    pub suit: &'static str,
    pub number: u8,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameStatsJson {
    pub rounds: u16,
    pub chapters: u16,
    pub battles: u16,
    pub cards_played: u16,
    pub ambitions_declared: u16,
    pub seizes: u16,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameStateJson {
    /// The TS state carries its variant's id string; the Rust state does not
    /// (the `VariantDef` travels alongside it), so it is synthesised here.
    pub variant: String,
    pub players: u8,
    pub chapter: u8,
    pub phase: &'static str,
    pub initiative: u8,
    pub initiative_seized: bool,
    pub systems: Vec<SystemStateJson>,
    pub player_states: Vec<PlayerStateJson>,
    pub supply: ByResourceJson<u8>,
    pub cartel: ByResourceJson<u8>,
    pub court: Vec<CourtSlotJson>,
    pub court_deck: Vec<u8>,
    pub court_discard: Vec<u8>,
    pub action_deck: Vec<u8>,
    pub action_discard: Vec<u8>,
    pub round: RoundStateJson,
    pub turn: Option<TurnStateJson>,
    pub battle: Option<BattleStateJson>,
    #[serde(rename = "move")]
    pub moving: Option<MoveStateJson>,
    pub declared: ByAmbitionJson<Vec<u8>>,
    pub available_markers: Vec<u8>,
    pub flipped: Vec<bool>,
    pub phantom: ByAmbitionJson<u8>,
    pub reinforcing: Option<u8>,
    pub unions: Vec<UnionJson>,
    pub pending_vox: Option<PendingVoxJson>,
    pub peek: Option<PeekJson>,
    pub revealed: Vec<u8>,
    pub declines: Vec<DeclineJson>,
    pub stats: GameStatsJson,
}

/// Project the Rust state into the TS `GameState` shape.
pub fn game_state_json(s: &GameState) -> GameStateJson {
    let n = s.players as usize;
    let seats = |a: &[u8; 4]| a[..n].to_vec();

    GameStateJson {
        variant: format!("{}p", s.players),
        players: s.players,
        chapter: s.chapter,
        phase: phase(s.phase),
        initiative: s.initiative.0,
        initiative_seized: s.initiative_seized,
        systems: (0..SYSTEM_COUNT)
            .map(|i| {
                let sys = &s.systems[i];
                SystemStateJson {
                    fresh: seats(&sys.fresh),
                    damaged: seats(&sys.damaged),
                    buildings: sys.buildings.iter().map(|b| (*b).into()).collect(),
                    out_of_play: sys.out_of_play,
                }
            })
            .collect(),
        player_states: s.player_states[..n]
            .iter()
            .map(|p| player_state_json(p, s.players))
            .collect(),
        supply: ByResourceJson::from_array(&s.supply),
        cartel: ByResourceJson::from_array(&s.cartel),
        court: s
            .court
            .iter()
            .map(|slot| CourtSlotJson {
                card: slot.card.0,
                agents: seats(&slot.agents),
            })
            .collect(),
        court_deck: s.court_deck.iter().map(|c| c.0).collect(),
        court_discard: s.court_discard.iter().map(|c| c.0).collect(),
        action_deck: s.action_deck.iter().map(|c| c.0).collect(),
        action_discard: s.action_discard.iter().map(|c| c.0).collect(),
        round: RoundStateJson {
            turn_index: s.round.turn_index,
            turn_order: s.round.turn_order.iter().map(|p| p.0).collect(),
            lead: s.round.lead.map(Into::into),
            lead_number: s.round.lead_number,
            played: s.round.played.iter().map(|c| (*c).into()).collect(),
            seized_by: s.round.seized_by.map(|p| p.0),
            consecutive_passes: s.round.consecutive_passes,
            ambition_declared: s.round.ambition_declared,
        },
        turn: s.turn.map(|t| TurnStateJson {
            player: t.player.0,
            mode: play_mode(t.mode),
            card: t.card.0,
            pips_left: t.pips_left,
            pip_actions: kinds(t.pip_actions),
            free_actions: t.free_actions.iter().map(|g| kinds(*g)).collect(),
            weapon_spent: t.weapon_spent,
            prelude_over: t.prelude_over,
            declared_this_turn: t.declared_this_turn,
            prelude_spent: ResourceType::ALL
                .iter()
                .flat_map(|r| {
                    core::iter::repeat_n(resource(*r), t.prelude_spent[r.as_index()] as usize)
                })
                .collect(),
            secured_this_prelude: t.secured_this_prelude.iter().map(|c| c.0).collect(),
            card_preludes_used: t.card_preludes_used.iter().map(|c| c.0).collect(),
        }),
        battle: s.battle.map(Into::into),
        moving: s.moving.map(Into::into),
        declared: ByAmbitionJson::from_fn(|a| s.declared[a.as_index()].iter().copied().collect()),
        available_markers: s.available_markers.iter().copied().collect(),
        flipped: s.flipped.to_vec(),
        phantom: ByAmbitionJson::from_fn(|a| s.phantom[a.as_index()]),
        reinforcing: s.reinforcing.map(|p| p.0),
        unions: s
            .unions
            .iter()
            .map(|u| UnionJson {
                card: u.card.0,
                player: u.player.0,
                target: u.target.0,
            })
            .collect(),
        pending_vox: s.pending_vox.map(|v| PendingVoxJson {
            card: v.card.0,
            player: v.player.0,
            resume: match v.resume {
                VoxResume::Actions => "actions",
                VoxResume::Battle => "battle",
            },
        }),
        peek: s.peek.map(|p| PeekJson {
            player: p.player.0,
            target: p.target.map(|t| t.0),
            resume: phase(p.resume),
        }),
        revealed: s.revealed.iter().map(|c| c.0).collect(),
        declines: s
            .declines
            .iter()
            .map(|d| DeclineJson {
                player: d.player.0,
                suit: suit(d.suit),
                number: d.number,
            })
            .collect(),
        stats: GameStatsJson {
            rounds: s.stats.rounds,
            chapters: s.stats.chapters,
            battles: s.stats.battles,
            cards_played: s.stats.cards_played,
            ambitions_declared: s.stats.ambitions_declared,
            seizes: s.stats.seizes,
        },
    }
}

// ---------------------------------------------------------------------------
// Action
// ---------------------------------------------------------------------------

/// The TS `Action` union: internally tagged on `t`, absent parameters omitted
/// (`undefined`) exactly where the TS code tests `x !== undefined`, and `null`
/// exactly where it tests `x === null` (`repair.building`, `peekTarget`).
#[derive(Serialize)]
#[serde(tag = "t", rename_all = "camelCase")]
pub enum ActionJson {
    Lead {
        card: u8,
    },
    Follow {
        card: u8,
        mode: &'static str,
    },
    PassInitiative,
    Mulligan {
        take: bool,
    },
    DeclareAmbition {
        ambition: &'static str,
    },
    Seize {
        card: u8,
    },
    SpendResource {
        slot: u8,
    },
    SpendResourceAs {
        slot: u8,
        #[serde(rename = "as")]
        spend_as: &'static str,
    },
    CardPrelude {
        card: u8,
        #[serde(skip_serializing_if = "Option::is_none")]
        system: Option<u8>,
        #[serde(skip_serializing_if = "Option::is_none")]
        slot: Option<u8>,
        #[serde(skip_serializing_if = "Option::is_none")]
        target: Option<u8>,
        #[serde(rename = "takeCard", skip_serializing_if = "Option::is_none")]
        take_card: Option<u8>,
        #[serde(skip_serializing_if = "Option::is_none")]
        played: Option<u8>,
        #[serde(skip_serializing_if = "Option::is_none")]
        cards: Option<Vec<u8>>,
    },
    BeginActions,
    Tax {
        system: u8,
        building: u8,
    },
    BuildShip {
        system: u8,
        building: u8,
    },
    BuildBuilding {
        system: u8,
        kind: &'static str,
    },
    CardAction {
        card: u8,
        name: &'static str,
        #[serde(skip_serializing_if = "Option::is_none")]
        gain: Option<Vec<&'static str>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        count: Option<u8>,
        #[serde(skip_serializing_if = "Option::is_none")]
        slot: Option<u8>,
        #[serde(skip_serializing_if = "Option::is_none")]
        system: Option<u8>,
        #[serde(skip_serializing_if = "Option::is_none")]
        building: Option<u8>,
        #[serde(rename = "giveSlot", skip_serializing_if = "Option::is_none")]
        give_slot: Option<u8>,
    },
    Move {
        from: u8,
        to: u8,
        ships: u8,
    },
    Catapult {
        to: u8,
        ships: u8,
    },
    CatapultStop,
    /// `building` is `null` for a ship repair — the TS UI tests for exactly
    /// that, so it must not be omitted.
    Repair {
        system: u8,
        building: Option<u8>,
    },
    Influence {
        slot: u8,
    },
    Secure {
        slot: u8,
    },
    Battle {
        system: u8,
        defender: u8,
        assault: u8,
        skirmish: u8,
        raid: u8,
    },
    EndTurn,
    AssignSelf {
        fresh: bool,
    },
    /// One variant for both TS arms; `target` discriminates and the unused
    /// field is omitted.
    AssignHit {
        target: &'static str,
        #[serde(skip_serializing_if = "Option::is_none")]
        fresh: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none")]
        building: Option<u8>,
    },
    RaidResource {
        slot: u8,
    },
    RaidCard {
        card: u8,
    },
    RaidDone,
    RerollSkirmish {
        count: u8,
    },
    /// `null` declines the peek.
    PeekTarget {
        target: Option<u8>,
    },
    PeekSwap {
        give: u8,
        take: u8,
    },
    PeekSwapSkip,
    Vox {
        #[serde(skip_serializing_if = "Option::is_none")]
        cluster: Option<u8>,
        #[serde(skip_serializing_if = "Option::is_none")]
        ambition: Option<&'static str>,
        #[serde(skip_serializing_if = "Option::is_none")]
        resource: Option<&'static str>,
        #[serde(skip_serializing_if = "Option::is_none")]
        system: Option<u8>,
        #[serde(skip_serializing_if = "Option::is_none")]
        building: Option<u8>,
        #[serde(skip_serializing_if = "Option::is_none")]
        seize: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none")]
        target: Option<u8>,
        #[serde(skip_serializing_if = "Option::is_none")]
        card: Option<u8>,
    },
    VoxSkip,
    Reinforce {
        system: u8,
    },
    /// Only emitted when `VariantDef::choose_placement` is on, which the
    /// browser does not turn on — the TypeScript `Action` union has no
    /// counterpart.
    PlaceResource {
        tier: u8,
    },
}

fn cards(list: Option<CardList>) -> Option<Vec<u8>> {
    list.map(|l| l.iter().map(|c| c.0).collect())
}

fn gains(list: Option<ResourceList>) -> Option<Vec<&'static str>> {
    list.map(|l| l.iter().map(|r| resource(*r)).collect())
}

fn sys(s: Option<SystemId>) -> Option<u8> {
    s.map(|s| s.0)
}

fn seat(p: Option<Player>) -> Option<u8> {
    p.map(|p| p.0)
}

fn court(c: Option<CourtCardId>) -> Option<u8> {
    c.map(|c| c.0)
}

/// Project one action into the TS `Action` shape.
pub fn action_json(a: Action) -> ActionJson {
    match a {
        Action::Lead { card } => ActionJson::Lead { card: card.0 },
        Action::Follow { card, mode } => ActionJson::Follow {
            card: card.0,
            mode: match mode {
                FollowMode::Surpass => "surpass",
                FollowMode::Copy => "copy",
                FollowMode::Pivot => "pivot",
            },
        },
        Action::PassInitiative => ActionJson::PassInitiative,
        Action::Mulligan { take } => ActionJson::Mulligan { take },
        Action::DeclareAmbition { ambition: a } => ActionJson::DeclareAmbition {
            ambition: ambition(a),
        },
        Action::Seize { card } => ActionJson::Seize { card: card.0 },
        Action::SpendResource { slot } => ActionJson::SpendResource { slot },
        Action::SpendResourceAs { slot, spend_as } => ActionJson::SpendResourceAs {
            slot,
            spend_as: resource(spend_as),
        },
        Action::CardPrelude { card, choice } => {
            let p = choice.parts();
            ActionJson::CardPrelude {
                card: card.0,
                system: sys(p.system),
                slot: p.slot,
                target: seat(p.target),
                take_card: court(p.take_card),
                played: p.played.map(|c| c.0),
                cards: cards(p.cards),
            }
        }
        Action::BeginActions => ActionJson::BeginActions,
        Action::Tax { system, building } => ActionJson::Tax {
            system: system.0,
            building,
        },
        Action::BuildShip { system, building } => ActionJson::BuildShip {
            system: system.0,
            building,
        },
        Action::BuildBuilding { system, kind } => ActionJson::BuildBuilding {
            system: system.0,
            kind: building_kind(kind),
        },
        Action::CardAction { card, choice } => {
            let p = choice.parts();
            ActionJson::CardAction {
                card: card.0,
                name: CardActionName::printed(choice.name()),
                gain: gains(p.gain),
                count: p.count,
                slot: p.slot,
                system: sys(p.system),
                building: p.building,
                give_slot: p.give_slot,
            }
        }
        Action::Move { from, to, ships } => ActionJson::Move {
            from: from.0,
            to: to.0,
            ships,
        },
        Action::Catapult { to, ships } => ActionJson::Catapult { to: to.0, ships },
        Action::CatapultStop => ActionJson::CatapultStop,
        Action::Repair { system, building } => ActionJson::Repair {
            system: system.0,
            building,
        },
        Action::Influence { slot } => ActionJson::Influence { slot },
        Action::Secure { slot } => ActionJson::Secure { slot },
        Action::Battle {
            system,
            defender,
            assault,
            skirmish,
            raid,
        } => ActionJson::Battle {
            system: system.0,
            defender: defender.0,
            assault,
            skirmish,
            raid,
        },
        Action::EndTurn => ActionJson::EndTurn,
        Action::AssignSelf { fresh } => ActionJson::AssignSelf { fresh },
        Action::AssignHit { target } => match target {
            HitTarget::Ship { fresh } => ActionJson::AssignHit {
                target: "ship",
                fresh: Some(fresh),
                building: None,
            },
            HitTarget::Building { building } => ActionJson::AssignHit {
                target: "building",
                fresh: None,
                building: Some(building),
            },
        },
        Action::RaidResource { slot } => ActionJson::RaidResource { slot },
        Action::RaidCard { card } => ActionJson::RaidCard { card: card.0 },
        Action::RaidDone => ActionJson::RaidDone,
        Action::RerollSkirmish { count } => ActionJson::RerollSkirmish { count },
        Action::PeekTarget { target } => ActionJson::PeekTarget {
            target: seat(target),
        },
        Action::PeekSwap { give, take } => ActionJson::PeekSwap {
            give: give.0,
            take: take.0,
        },
        Action::PeekSwapSkip => ActionJson::PeekSwapSkip,
        Action::Vox(choice) => {
            let p = choice.parts();
            ActionJson::Vox {
                cluster: p.cluster,
                ambition: p.ambition.map(ambition),
                resource: p.resource.map(resource),
                system: sys(p.system),
                building: p.building,
                seize: p.seize,
                target: seat(p.target),
                card: court(p.card),
            }
        }
        Action::VoxSkip => ActionJson::VoxSkip,
        Action::Reinforce { system } => ActionJson::Reinforce { system: system.0 },
        Action::PlaceResource { tier } => ActionJson::PlaceResource { tier },
    }
}

// ---------------------------------------------------------------------------
// VariantDef
// ---------------------------------------------------------------------------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemDefJson {
    pub id: u8,
    pub cluster: u8,
    pub slot: u8,
    pub kind: &'static str,
    pub planet_type: Option<&'static str>,
    pub building_slots: u8,
    pub adjacent: Vec<u8>,
    pub label: String,
}

#[derive(Serialize)]
pub struct ActionCardDefJson {
    pub id: u8,
    pub suit: &'static str,
    pub number: u8,
    pub pips: u8,
    /// `null`, an ambition name, or the "7"'s `'any'`.
    pub ambition: Option<&'static str>,
}

fn action_card_json(def: &ActionCardDef) -> ActionCardDefJson {
    ActionCardDefJson {
        id: def.id.0,
        suit: suit(def.suit),
        number: def.number,
        pips: def.pips,
        ambition: match def.ambition() {
            CardAmbition::None => None,
            CardAmbition::Any => Some("any"),
            CardAmbition::Some(a) => Some(ambition(a)),
        },
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CourtCardDefJson {
    pub id: u8,
    pub number: u8,
    pub name: &'static str,
    pub kind: &'static str,
    pub suit: Option<&'static str>,
    pub raid_cost: u8,
    /// The Rust port carries card rules, not card prose; the UI fills this
    /// in from its own static table. See the module docs.
    pub text: &'static str,
    pub discard_on_secure: bool,
}

#[derive(Serialize)]
pub struct MarkerSideJson {
    pub first: u8,
    pub second: u8,
}

#[derive(Serialize)]
pub struct AmbitionMarkerJson {
    pub blue: MarkerSideJson,
    pub orange: MarkerSideJson,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VariantJson {
    pub id: String,
    pub name: String,
    pub players: u8,
    pub systems: Vec<SystemDefJson>,
    pub action_deck: Vec<ActionCardDefJson>,
    pub court_deck: Vec<CourtCardDefJson>,
    pub ambition_markers: Vec<AmbitionMarkerJson>,
    pub court_row_size: u8,
    pub power_threshold: u8,
    pub max_chapters: u8,
    pub hand_size: u8,
}

/// Project the variant into the TS `VariantDef` shape.
pub fn variant_json(v: &VariantDef) -> VariantJson {
    VariantJson {
        id: format!("{}p", v.players),
        name: format!("{} players", v.players),
        players: v.players,
        systems: v
            .systems
            .iter()
            .map(|d| SystemDefJson {
                id: d.id.0,
                cluster: d.cluster,
                slot: d.slot,
                kind: match d.kind {
                    SystemKind::Gate => "gate",
                    SystemKind::Planet => "planet",
                },
                planet_type: d.planet_type.map(resource),
                building_slots: d.building_slots,
                adjacent: d.adjacent.iter().map(|s| s.0).collect(),
                label: d.to_string(),
            })
            .collect(),
        action_deck: v
            .action_deck
            .iter()
            .map(|id| action_card_json(action_card(*id)))
            .collect(),
        court_deck: v
            .court_deck
            .iter()
            .map(|id| {
                let c = court_card(*id);
                CourtCardDefJson {
                    id: c.id.0,
                    number: c.number,
                    name: c.name,
                    kind: match c.kind {
                        CourtCardKind::Guild => "guild",
                        CourtCardKind::Vox => "vox",
                    },
                    suit: c.suit.map(resource),
                    raid_cost: c.raid_cost,
                    text: "",
                    discard_on_secure: c.discard_on_secure(),
                }
            })
            .collect(),
        ambition_markers: (0..v.ambition_markers.len())
            .map(|i| {
                let blue = marker_value(&v.ambition_markers, i, false);
                let orange = marker_value(&v.ambition_markers, i, true);
                AmbitionMarkerJson {
                    blue: MarkerSideJson {
                        first: blue.first,
                        second: blue.second,
                    },
                    orange: MarkerSideJson {
                        first: orange.first,
                        second: orange.second,
                    },
                }
            })
            .collect(),
        court_row_size: v.court_row_size,
        power_threshold: v.power_threshold,
        max_chapters: v.max_chapters,
        hand_size: v.hand_size,
    }
}

// ---------------------------------------------------------------------------
// Small results
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct StandingJson {
    pub player: u8,
    pub power: u8,
    pub rank: u8,
}

pub fn standings_json(rows: &InlineVec<Standing, 4>) -> Vec<StandingJson> {
    rows.iter()
        .map(|r| StandingJson {
            player: r.player.0,
            power: r.power,
            rank: r.rank,
        })
        .collect()
}

/// What the game needs next, in the TS `PendingNode` spirit — but carrying
/// only the count of legal actions, since the list is fetched separately.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingJson {
    /// `"over" | "chance" | "decision"`.
    pub kind: &'static str,
    pub player: Option<u8>,
    pub n_actions: usize,
}

/// Ambition tallies per seat, in [`AmbitionId::ALL`] order — the numbers the
/// Ambitions panel shows, computed by the engine rather than re-derived in
/// TypeScript.
pub fn ambition_counts_json(s: &GameState) -> Vec<Vec<u8>> {
    (0..s.players as usize)
        .map(|p| {
            AmbitionId::ALL
                .iter()
                .map(|a| arcs_engine::ambition_count(&s.player_states[p], *a))
                .collect()
        })
        .collect()
}

/// `TrophyKind` is only referenced through its name table; this keeps the
/// table and the enum from drifting apart.
const _: () = assert!(TROPHY_KIND_NAMES.len() == TrophyKind::COUNT);
