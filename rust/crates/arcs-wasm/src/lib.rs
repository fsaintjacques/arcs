//! Browser bindings: one [`Session`] type, JSON in and out.
//!
//! This is the wasm half of the plan's §3 decision-process API. The surface is
//! deliberately the same one PyO3 will wrap: ask what the game needs
//! (`pending`), read the legal list, **choose by index** (`apply`), resolve
//! chance, snapshot and restore. Nothing here can play an unoffered action,
//! because there is no way to name one.
//!
//! Two things are wasm-specific:
//!
//! - **TS-shaped JSON.** `state_json`, `legal` and `variant_json` emit the
//!   shapes in `src/engine/types.ts`, so the existing React components render
//!   the Rust engine without being rewritten. See [`ts_json`].
//! - **A JS clock.** `wasm32-unknown-unknown` has no `std::time`, so the
//!   search agents' wall-clock budget is fed `Date.now()` through
//!   [`arcs_agents::AgentOpts::clock`]. Without it `mcts2-play` would panic
//!   the moment it read the clock.
//!
//! Snapshots are the other reason the browser wants Rust: `GameState` is
//! `Copy`, so the UI's undo stack is a `Vec<GameState>` and a rewind is a
//! memcpy — no replaying an RNG stream forward to reconstruct a past position.

pub mod ts_json;

use arcs_agents::{AGENT_NAMES, Agent, AgentCtx, AgentOpts, Clock, make_agent};
use arcs_engine::{
    Action, GameState, Pending, Player, SetupMode, SplitMix64, VariantDef, apply_action_mut,
    encode_action, get_pending, legal_actions, make_variant, new_game, observe, resolve_chance_mut,
    standings,
};
use wasm_bindgen::prelude::*;

use ts_json::PendingJson;

// ---------------------------------------------------------------------------
// The JS clock
// ---------------------------------------------------------------------------

#[cfg(target_family = "wasm")]
mod js_clock {
    use wasm_bindgen::prelude::*;

    #[wasm_bindgen]
    extern "C" {
        #[wasm_bindgen(js_namespace = Date, js_name = now)]
        fn date_now() -> f64;
    }

    pub fn now_ms() -> u64 {
        date_now() as u64
    }
}

/// The clock the search agents should use, or `None` to leave them on
/// `std::time` — which is what a native build (`cargo test`) has anyway.
fn clock() -> Option<Clock> {
    #[cfg(target_family = "wasm")]
    {
        Some(Clock(js_clock::now_ms))
    }
    #[cfg(not(target_family = "wasm"))]
    {
        None
    }
}

/// Installed on module load: without it a Rust panic reaches the console as
/// `unreachable executed`, which says nothing about what went wrong.
#[wasm_bindgen(start)]
pub fn start() {
    console_error_panic_hook::set_once();
}

/// Why a call was refused.
///
/// A plain owned string rather than `JsError`, because building a `JsError`
/// means calling into JavaScript — which a native `cargo test` cannot do, and
/// the error paths are exactly what the tests need to exercise. The conversion
/// to a thrown `Error` happens in the generated shim, on wasm only.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct SessionError(String);

impl SessionError {
    fn new(message: impl Into<String>) -> Self {
        SessionError(message.into())
    }
}

impl core::fmt::Display for SessionError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.0)
    }
}

impl core::error::Error for SessionError {}

impl From<SessionError> for JsValue {
    fn from(e: SessionError) -> JsValue {
        JsError::new(&e.0).into()
    }
}

/// The agent names the browser may put in a seat.
#[wasm_bindgen(js_name = agentNames)]
pub fn agent_names() -> Vec<String> {
    AGENT_NAMES.iter().map(|s| (*s).to_string()).collect()
}

// ---------------------------------------------------------------------------
// Session
// ---------------------------------------------------------------------------

/// A saved position: the state, plus every RNG that would otherwise re-draw
/// different dice on the way forward. Restoring one puts the game back exactly
/// as it stood, which is what the UI's undo needs.
struct Snapshot {
    state: GameState,
    chance: SplitMix64,
    agent_rngs: Vec<SplitMix64>,
}

/// One game, driven from JavaScript.
#[wasm_bindgen]
pub struct Session {
    variant: VariantDef,
    state: GameState,
    chance: SplitMix64,
    /// Cached for the current state; the index `apply` takes is an index here.
    legal: Vec<Action>,
    snapshots: Vec<Snapshot>,
    agents: Vec<Option<Box<dyn Agent>>>,
    agent_rngs: Vec<SplitMix64>,
}

/// Per-seat agent streams, forked from the game seed so a seat's randomness
/// is independent of the chance stream and of the other seats.
fn fork_agent_rngs(seed: u64, players: u8) -> Vec<SplitMix64> {
    (0..players)
        .map(|p| SplitMix64::new(seed ^ (0x9E37_79B9_7F4A_7C15u64.wrapping_mul(p as u64 + 1))))
        .collect()
}

#[wasm_bindgen]
impl Session {
    /// Start a game. `setup_index` picks the opening board and `seed` drives
    /// every chance node; `mode` is `"deck"` (one of the printed setup cards)
    /// or `"draw"` (a generated legal opening).
    #[wasm_bindgen(constructor)]
    pub fn new(
        players: u8,
        setup_index: u32,
        seed: u32,
        mode: &str,
    ) -> Result<Session, SessionError> {
        if !(2..=4).contains(&players) {
            return Err(SessionError::new("players must be 2, 3 or 4"));
        }
        let mode = match mode {
            "deck" => SetupMode::Deck,
            "draw" => SetupMode::Draw,
            other => return Err(SessionError::new(format!("unknown setup mode {other}"))),
        };
        let seed = seed as u64;
        let variant = make_variant(players, setup_index as u64, mode);
        let mut chance = SplitMix64::new(seed);
        let state = new_game(&variant, &mut chance, setup_index as u64, mode);
        let mut session = Session {
            variant,
            state,
            chance,
            legal: Vec::new(),
            snapshots: Vec::new(),
            agents: (0..players).map(|_| None).collect(),
            agent_rngs: fork_agent_rngs(seed, players),
        };
        session.refresh();
        Ok(session)
    }

    /// Recompute the legal list for the current state. Cheap enough to do
    /// eagerly, and it keeps `apply(index)` honest.
    fn refresh(&mut self) {
        self.legal.clear();
        if let Pending::Decision { .. } = get_pending(&self.state, &self.variant) {
            legal_actions(&self.state, &self.variant, &mut self.legal);
        }
    }

    /// What the game needs next: `{ kind, player, nActions }`.
    pub fn pending(&self) -> String {
        let node = match get_pending(&self.state, &self.variant) {
            Pending::Over => PendingJson {
                kind: "over",
                player: None,
                n_actions: 0,
            },
            Pending::Chance => PendingJson {
                kind: "chance",
                player: None,
                n_actions: 0,
            },
            Pending::Decision { player } => PendingJson {
                kind: "decision",
                player: Some(player.0),
                n_actions: self.legal.len(),
            },
        };
        json(&node)
    }

    /// The legal actions, in the TS `Action` shape, as a JSON array. Empty at
    /// a chance node or at game over.
    pub fn legal(&self) -> String {
        let list: Vec<_> = self
            .legal
            .iter()
            .map(|a| ts_json::action_json(*a))
            .collect();
        json(&list)
    }

    /// The same list as canonical `encode_action` keys — the stable names for
    /// logs, tests and cross-engine comparison.
    #[wasm_bindgen(js_name = legalKeys)]
    pub fn legal_keys(&self) -> Vec<String> {
        self.legal.iter().map(|a| encode_action(*a)).collect()
    }

    /// Play the legal action at `index`.
    pub fn apply(&mut self, index: usize) -> Result<(), SessionError> {
        let action = *self
            .legal
            .get(index)
            .ok_or_else(|| SessionError::new("no legal action at that index"))?;
        apply_action_mut(&mut self.state, &self.variant, action)
            .map_err(|e| SessionError::new(e.to_string()))?;
        self.refresh();
        Ok(())
    }

    /// Resolve the pending chance node (a deal, or a battle roll).
    #[wasm_bindgen(js_name = resolveChance)]
    pub fn resolve_chance(&mut self) -> Result<(), SessionError> {
        resolve_chance_mut(&mut self.state, &self.variant, &mut self.chance)
            .map_err(|e| SessionError::new(e.to_string()))?;
        self.refresh();
        Ok(())
    }

    /// The full state, in the TS `GameState` shape.
    #[wasm_bindgen(js_name = stateJson)]
    pub fn state_json(&self) -> String {
        json(&ts_json::game_state_json(&self.state))
    }

    /// The variant, in the TS `VariantDef` shape.
    #[wasm_bindgen(js_name = variantJson)]
    pub fn variant_json(&self) -> String {
        json(&ts_json::variant_json(&self.variant))
    }

    /// One player's legal view — the introspection channel, in the Rust
    /// state's own shape rather than the TS one.
    #[wasm_bindgen(js_name = observeJson)]
    pub fn observe_json(&self, player: u8) -> Result<String, SessionError> {
        if player >= self.state.players {
            return Err(SessionError::new("no such seat"));
        }
        Ok(json(&observe(&self.state, &self.variant, Player(player))))
    }

    /// Final standings, best first: `[{ player, power, rank }]`.
    pub fn standings(&self) -> String {
        json(&ts_json::standings_json(&standings(&self.state)))
    }

    /// Ambition tallies per seat, in tycoon/tyrant/warlord/keeper/empath
    /// order.
    #[wasm_bindgen(js_name = ambitionCounts)]
    pub fn ambition_counts(&self) -> String {
        json(&ts_json::ambition_counts_json(&self.state))
    }

    /// Save the position and return its handle. `GameState` is `Copy`, so
    /// this is a memcpy plus the RNG words.
    pub fn snapshot(&mut self) -> usize {
        self.snapshots.push(Snapshot {
            state: self.state,
            chance: self.chance,
            agent_rngs: self.agent_rngs.clone(),
        });
        self.snapshots.len() - 1
    }

    /// Put the game back to a saved position, dice and deals included.
    pub fn restore(&mut self, id: usize) -> Result<(), SessionError> {
        let snap = self
            .snapshots
            .get(id)
            .ok_or_else(|| SessionError::new("no such snapshot"))?;
        self.state = snap.state;
        self.chance = snap.chance;
        self.agent_rngs = snap.agent_rngs.clone();
        self.refresh();
        Ok(())
    }

    /// Drop every snapshot from `len` on — how the UI's undo stack forgets
    /// positions it can no longer reach.
    #[wasm_bindgen(js_name = truncateSnapshots)]
    pub fn truncate_snapshots(&mut self, len: usize) {
        self.snapshots.truncate(len);
    }

    /// How many snapshots are stored.
    #[wasm_bindgen(js_name = snapshotCount)]
    pub fn snapshot_count(&self) -> usize {
        self.snapshots.len()
    }

    /// Put a registry agent in a seat, or `undefined` to make it human.
    #[wasm_bindgen(js_name = setAgent)]
    pub fn set_agent(&mut self, seat: u8, name: Option<String>) -> Result<(), SessionError> {
        let slot = self
            .agents
            .get_mut(seat as usize)
            .ok_or_else(|| SessionError::new("no such seat"))?;
        *slot = match name {
            None => None,
            Some(name) => {
                let opts = AgentOpts {
                    clock: clock(),
                    ..AgentOpts::default()
                };
                Some(make_agent(&name, &opts).map_err(|e| SessionError::new(e.to_string()))?)
            }
        };
        Ok(())
    }

    /// Run the seat-on-turn's agent and return its choice, as an index into
    /// the legal list. Errors when a human owes the decision.
    #[wasm_bindgen(js_name = botChoose)]
    pub fn bot_choose(&mut self) -> Result<usize, SessionError> {
        let Pending::Decision { player } = get_pending(&self.state, &self.variant) else {
            return Err(SessionError::new("not a decision node"));
        };
        let seat = player.as_index();
        let Some(agent) = self.agents.get_mut(seat).and_then(Option::as_mut) else {
            return Err(SessionError::new("that seat is human"));
        };
        let obs = observe(&self.state, &self.variant, player);
        let mut ctx = AgentCtx {
            variant: &self.variant,
            rng: self.agent_rngs[seat],
            player,
        };
        let index = agent.choose(&obs, &self.legal, &mut ctx);
        self.agent_rngs[seat] = ctx.rng;
        if index >= self.legal.len() {
            return Err(SessionError::new("agent chose out of range"));
        }
        Ok(index)
    }
}

/// Serialising these shapes cannot fail — every field is a plain number,
/// string, bool or list of the same — so a failure is a bug in this crate,
/// not something the caller can act on.
fn json<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_string(value).expect("TS-shaped JSON always serialises")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn session() -> Session {
        Session::new(3, 0, 7, "deck").expect("a 3-player game starts")
    }

    /// Drive a game to the end, letting greedy play every seat.
    fn play_out(s: &mut Session) -> usize {
        for seat in 0..3 {
            s.set_agent(seat, Some("greedy".into())).unwrap();
        }
        let mut steps = 0;
        loop {
            let node: Value = serde_json::from_str(&s.pending()).unwrap();
            match node["kind"].as_str().unwrap() {
                "over" => return steps,
                "chance" => s.resolve_chance().unwrap(),
                _ => {
                    let i = s.bot_choose().unwrap();
                    s.apply(i).unwrap();
                }
            }
            steps += 1;
            assert!(steps < 200_000, "game did not terminate");
        }
    }

    /// The contract the React UI depends on: the emitted state has the field
    /// names `src/engine/types.ts` declares, spelled the way TypeScript spells
    /// them. A rename on either side breaks the browser silently, so it is
    /// asserted rather than eyeballed.
    #[test]
    fn state_json_matches_the_typescript_game_state() {
        let mut s = session();
        s.resolve_chance().unwrap(); // deal chapter 1
        let v: Value = serde_json::from_str(&s.state_json()).unwrap();
        let obj = v.as_object().unwrap();

        let expected = [
            "variant",
            "players",
            "chapter",
            "phase",
            "initiative",
            "initiativeSeized",
            "systems",
            "playerStates",
            "supply",
            "cartel",
            "court",
            "courtDeck",
            "courtDiscard",
            "actionDeck",
            "actionDiscard",
            "round",
            "turn",
            "battle",
            "move",
            "declared",
            "availableMarkers",
            "flipped",
            "phantom",
            "reinforcing",
            "unions",
            "pendingVox",
            "peek",
            "revealed",
            "declines",
            "stats",
        ];
        for key in expected {
            assert!(obj.contains_key(key), "GameState is missing {key}");
        }
        let mut extra: Vec<&str> = obj.keys().map(String::as_str).collect();
        extra.retain(|k| !expected.contains(k));
        assert!(
            extra.is_empty(),
            "GameState has unexpected fields {extra:?}"
        );

        // Records are objects keyed by the TS string unions.
        for key in ["material", "fuel", "weapon", "relic", "psionic"] {
            assert!(v["supply"][key].is_number(), "supply.{key}");
            assert!(v["cartel"][key].is_number(), "cartel.{key}");
        }
        for key in ["tycoon", "tyrant", "warlord", "keeper", "empath"] {
            assert!(v["declared"][key].is_array(), "declared.{key}");
            assert!(v["phantom"][key].is_number(), "phantom.{key}");
        }

        // Per-seat arrays are `players` long, as in TS — not MAX_SEATS.
        assert_eq!(v["playerStates"].as_array().unwrap().len(), 3);
        assert_eq!(v["systems"][0]["fresh"].as_array().unwrap().len(), 3);
        assert_eq!(v["court"][0]["agents"].as_array().unwrap().len(), 3);

        // Nullable fields are null, not absent: the UI tests them with `===`.
        assert!(v["battle"].is_null() && v["move"].is_null());
        assert!(v["pendingVox"].is_null() && v["peek"].is_null());

        let p = &v["playerStates"][0];
        for key in [
            "power",
            "resources",
            "outrage",
            "guildCards",
            "trophies",
            "captives",
            "agentsSupply",
            "shipsSupply",
            "starportsSupply",
            "citiesUsed",
            "hand",
        ] {
            assert!(p.get(key).is_some(), "PlayerState is missing {key}");
        }
        assert!(p["outrage"]["material"].is_boolean());
        // Six slots, open or covered, exactly like the TS player board.
        assert_eq!(p["resources"].as_array().unwrap().len(), 6);

        let sys = &v["systems"][0];
        for key in ["fresh", "damaged", "buildings", "outOfPlay"] {
            assert!(sys.get(key).is_some(), "SystemState is missing {key}");
        }
        for key in ["turnIndex", "turnOrder", "lead", "leadNumber", "played"] {
            assert!(v["round"].get(key).is_some(), "RoundState is missing {key}");
        }
        for key in [
            "rounds",
            "chapters",
            "battles",
            "cardsPlayed",
            "ambitionsDeclared",
            "seizes",
        ] {
            assert!(v["stats"].get(key).is_some(), "GameStats is missing {key}");
        }
    }

    #[test]
    fn variant_json_matches_the_typescript_variant_def() {
        let s = session();
        let v: Value = serde_json::from_str(&s.variant_json()).unwrap();
        for key in [
            "id",
            "name",
            "players",
            "systems",
            "actionDeck",
            "courtDeck",
            "ambitionMarkers",
            "courtRowSize",
            "powerThreshold",
            "maxChapters",
            "handSize",
        ] {
            assert!(v.get(key).is_some(), "VariantDef is missing {key}");
        }
        assert_eq!(v["systems"].as_array().unwrap().len(), 24);
        let sys = &v["systems"][1];
        for key in [
            "id",
            "cluster",
            "slot",
            "kind",
            "planetType",
            "buildingSlots",
            "adjacent",
            "label",
        ] {
            assert!(sys.get(key).is_some(), "SystemDef is missing {key}");
        }
        // The board draws gates from ids and planets from `planetType`.
        assert_eq!(v["systems"][0]["kind"], "gate");
        assert!(v["systems"][0]["planetType"].is_null());
        assert_eq!(v["systems"][1]["kind"], "planet");
        assert!(v["systems"][1]["planetType"].is_string());
        assert_eq!(
            v["ambitionMarkers"][0]["blue"]["first"].as_u64().unwrap(),
            5
        );
        // The 3-player deck is the 20 cards numbered 2-6.
        assert_eq!(v["actionDeck"].as_array().unwrap().len(), 20);
        assert_eq!(v["courtDeck"].as_array().unwrap().len(), 31);
    }

    /// Every action the engine can offer must project into a TS action with a
    /// `t` tag the UI's `describeAction` switch knows. Playing 3 full games
    /// reaches the great majority of them; the tag check is the invariant.
    #[test]
    fn every_offered_action_projects_to_a_tagged_object() {
        const TAGS: [&str; 30] = [
            "lead",
            "follow",
            "passInitiative",
            "mulligan",
            "declareAmbition",
            "seize",
            "spendResource",
            "spendResourceAs",
            "cardPrelude",
            "beginActions",
            "tax",
            "buildShip",
            "buildBuilding",
            "cardAction",
            "move",
            "catapult",
            "catapultStop",
            "repair",
            "influence",
            "secure",
            "battle",
            "endTurn",
            "assignSelf",
            "assignHit",
            "raidResource",
            "raidCard",
            "raidDone",
            "rerollSkirmish",
            "peekTarget",
            "peekSwap",
        ];
        // `peekSwapSkip`, `vox`, `voxSkip` and `reinforce` are rare enough
        // that a handful of games need not reach them; they are covered by
        // the exhaustive match in `ts_json::action_json`.
        let mut s = Session::new(4, 3, 11, "deck").unwrap();
        for seat in 0..4 {
            s.set_agent(seat, Some("greedy".into())).unwrap();
        }
        let mut seen = std::collections::BTreeSet::new();
        let mut steps = 0;
        loop {
            let node: Value = serde_json::from_str(&s.pending()).unwrap();
            match node["kind"].as_str().unwrap() {
                "over" => break,
                "chance" => s.resolve_chance().unwrap(),
                _ => {
                    let list: Vec<Value> = serde_json::from_str(&s.legal()).unwrap();
                    assert_eq!(list.len(), node["nActions"].as_u64().unwrap() as usize);
                    for a in &list {
                        let tag = a["t"].as_str().expect("every action carries a `t` tag");
                        assert!(
                            TAGS.contains(&tag)
                                || matches!(tag, "peekSwapSkip" | "vox" | "voxSkip" | "reinforce"),
                            "unknown action tag {tag}"
                        );
                        seen.insert(tag.to_string());
                    }
                    let i = s.bot_choose().unwrap();
                    s.apply(i).unwrap();
                }
            }
            steps += 1;
            assert!(steps < 200_000);
        }
        // A real game exercises the common half of the vocabulary.
        for tag in ["lead", "follow", "beginActions", "endTurn"] {
            assert!(seen.contains(tag), "a whole game never offered {tag}");
        }
    }

    /// `repair.building` and `peekTarget.target` are `null` rather than
    /// omitted, because the UI distinguishes them with `=== null`; optional
    /// card parameters are omitted rather than null, because it tests those
    /// with `!== undefined`.
    #[test]
    fn optional_action_fields_follow_the_typescript_conventions() {
        use arcs_engine::{Action, SystemId};
        let repair = serde_json::to_value(ts_json::action_json(Action::Repair {
            system: SystemId(4),
            building: None,
        }))
        .unwrap();
        assert!(repair["building"].is_null());

        let peek = serde_json::to_value(ts_json::action_json(Action::PeekTarget { target: None }))
            .unwrap();
        assert!(peek["target"].is_null());

        let prelude = serde_json::to_value(ts_json::action_json(Action::CardPrelude {
            card: arcs_engine::CourtCardId(3),
            choice: arcs_engine::PreludeChoice::Bare,
        }))
        .unwrap();
        let obj = prelude.as_object().unwrap();
        assert_eq!(obj.len(), 2, "only `t` and `card` survive: {obj:?}");
    }

    #[test]
    fn apply_only_accepts_offered_indices() {
        let mut s = session();
        assert!(s.apply(0).is_err(), "the deal is a chance node");
        s.resolve_chance().unwrap();
        let n = s.legal_keys().len();
        assert!(n > 0);
        assert!(s.apply(n).is_err(), "one past the end is not offered");
        assert!(s.apply(0).is_ok());
    }

    /// The undo the UI is built on: a snapshot replays the same future,
    /// dice and deals included.
    #[test]
    fn snapshots_restore_the_state_and_the_dice() {
        let mut a = session();
        a.set_agent(0, Some("greedy".into())).unwrap();
        a.set_agent(1, Some("greedy".into())).unwrap();
        a.set_agent(2, Some("greedy".into())).unwrap();
        for _ in 0..40 {
            match serde_json::from_str::<Value>(&a.pending()).unwrap()["kind"]
                .as_str()
                .unwrap()
            {
                "over" => break,
                "chance" => a.resolve_chance().unwrap(),
                _ => {
                    let i = a.bot_choose().unwrap();
                    a.apply(i).unwrap();
                }
            }
        }
        let id = a.snapshot();
        let mark = a.state_json();

        // Play on, then rewind and replay: the same positions come back.
        let mut forward = Vec::new();
        for _ in 0..30 {
            match serde_json::from_str::<Value>(&a.pending()).unwrap()["kind"]
                .as_str()
                .unwrap()
            {
                "over" => break,
                "chance" => a.resolve_chance().unwrap(),
                _ => {
                    let i = a.bot_choose().unwrap();
                    a.apply(i).unwrap();
                }
            }
            forward.push(a.state_json());
        }
        a.restore(id).unwrap();
        assert_eq!(a.state_json(), mark, "restore rewinds the position");

        let mut again = Vec::new();
        for _ in 0..30 {
            match serde_json::from_str::<Value>(&a.pending()).unwrap()["kind"]
                .as_str()
                .unwrap()
            {
                "over" => break,
                "chance" => a.resolve_chance().unwrap(),
                _ => {
                    let i = a.bot_choose().unwrap();
                    a.apply(i).unwrap();
                }
            }
            again.push(a.state_json());
        }
        assert_eq!(forward, again, "the rewound game replays identically");
    }

    #[test]
    fn a_session_plays_a_whole_game_and_ranks_the_seats() {
        let mut s = session();
        let steps = play_out(&mut s);
        assert!(steps > 100, "a full game takes more than {steps} steps");

        let node: Value = serde_json::from_str(&s.pending()).unwrap();
        assert_eq!(node["kind"], "over");
        assert_eq!(s.legal(), "[]");

        let table: Vec<Value> = serde_json::from_str(&s.standings()).unwrap();
        assert_eq!(table.len(), 3);
        assert_eq!(table[0]["rank"], 0);
        let powers: Vec<u64> = table.iter().map(|r| r["power"].as_u64().unwrap()).collect();
        assert!(powers.windows(2).all(|w| w[0] >= w[1]), "sorted by Power");

        let counts: Vec<Vec<u8>> = serde_json::from_str(&s.ambition_counts()).unwrap();
        assert_eq!(counts.len(), 3);
        assert_eq!(counts[0].len(), 5);
    }

    #[test]
    fn seats_are_human_until_an_agent_is_put_in_them() {
        let mut s = session();
        s.resolve_chance().unwrap();
        assert!(s.bot_choose().is_err(), "an empty seat is a human seat");
        s.set_agent(0, Some("greedy".into())).unwrap();
        s.set_agent(1, Some("greedy".into())).unwrap();
        s.set_agent(2, Some("greedy".into())).unwrap();
        assert!(s.bot_choose().is_ok());
        assert!(s.set_agent(0, Some("nope".into())).is_err());
        assert!(s.set_agent(9, Some("greedy".into())).is_err());
    }

    #[test]
    fn legal_keys_are_the_canonical_encoding() {
        let mut s = session();
        s.resolve_chance().unwrap();
        let keys = s.legal_keys();
        let list: Vec<Value> = serde_json::from_str(&s.legal()).unwrap();
        assert_eq!(keys.len(), list.len());
        assert!(keys.iter().all(|k| !k.is_empty()));
        // The opening decision is a lead or a pass, both of which encode
        // with their own prefixes.
        assert!(keys.iter().all(|k| k.starts_with("ld:") || k == "pi"));
    }

    #[test]
    fn observations_hide_rival_hands() {
        let mut s = session();
        s.resolve_chance().unwrap();
        let obs: Value = serde_json::from_str(&s.observe_json(0).unwrap()).unwrap();
        assert_eq!(obs["player"], 0);
        assert!(
            !obs["state"]["player_states"][0]["hand"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        assert!(
            obs["state"]["player_states"][1]["hand"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        assert!(s.observe_json(7).is_err());
    }

    #[test]
    fn bad_configurations_are_refused() {
        assert!(Session::new(1, 0, 1, "deck").is_err());
        assert!(Session::new(5, 0, 1, "deck").is_err());
        assert!(Session::new(3, 0, 1, "shuffle").is_err());
        assert!(Session::new(3, 0, 1, "draw").is_ok());
    }
}
