//! The mutable game state, mirroring `src/engine/types.ts` (`GameState` and
//! friends) as one flat `Copy` struct.
//!
//! Every collection is statically bounded and stored inline, so `Clone` is a
//! memcpy and search states live on the stack — this kills the TS engine's #1
//! hot path, `cloneState`. Differences from the TS shape, all layout-only:
//!
//! - `variant: string` is dropped: the [`crate::setup::VariantDef`] travels
//!   alongside the state in every API call, so the id string bought nothing.
//! - `trophies`/`captives` lists become count matrices (owner x kind). The TS
//!   engine only ever counts them — except `pressgang`/`execute` popping in
//!   capture order, which only decides *whose* supply an agent returns to;
//!   the Rust port resolves that deterministically (highest owner first, R3).
//! - `Record<ResourceType, T>` becomes `[T; 5]`, `Record<AmbitionId, T>`
//!   becomes `[T; 5]`, both indexed by `as_index()`.
//! - `Building` packs into one byte; a system's ships are `[u8; 4]` per
//!   freshness.
//! - `PlayerState` carries `leader`/`lore` now (always empty in the base
//!   game) so the struct never changes shape for Leaders & Lore.

use crate::ambitions::AMBITION_MARKER_COUNT;
use crate::cards::ACTION_CARD_COUNT;
use crate::court::COURT_CARD_COUNT;
use crate::dice::DICE_PER_TYPE;
use crate::inline_vec::InlineVec;
use crate::map::SYSTEM_COUNT;
use crate::player_board::{
    AGENTS, MAX_RESOURCE_SLOTS, SHIPS, STARPORTS, open_resource_slots, raid_cost,
};
use crate::setup::{MAX_PLAYERS, VariantDef};
use crate::types::{
    ActionCardId, ActionKind, AmbitionId, BuildingKind, ByDieType, ByResource, CourtCardId, Phase,
    PlayMode, Player, ResourceType, Suit, SystemId, id_newtype, index_enum,
};

// ---------------------------------------------------------------------------
// Small packed pieces
// ---------------------------------------------------------------------------

id_newtype! {
    /// A leader card id (Leaders & Lore; unused in the base game).
    pub struct LeaderId
}

id_newtype! {
    /// A lore card id (Leaders & Lore; unused in the base game).
    pub struct LoreId
}

/// A building on the map, packed into one byte:
/// bits 0-1 player, bit 2 kind, bit 3 damaged, bit 4 taxed, bit 5 built.
///
/// Presence is the [`InlineVec`] length, so no present bit is needed.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(transparent))]
pub struct Building(pub u8);

impl Building {
    #[inline]
    pub fn new(player: Player, kind: BuildingKind, damaged: bool) -> Self {
        let mut b = (player.0 & 0b11) | ((kind as u8) << 2);
        if damaged {
            b |= 1 << 3;
        }
        Building(b)
    }

    #[inline]
    pub fn player(self) -> Player {
        Player(self.0 & 0b11)
    }

    #[inline]
    pub fn kind(self) -> BuildingKind {
        if self.0 & (1 << 2) == 0 {
            BuildingKind::City
        } else {
            BuildingKind::Starport
        }
    }

    #[inline]
    pub fn damaged(self) -> bool {
        self.0 & (1 << 3) != 0
    }

    #[inline]
    pub fn set_damaged(&mut self, damaged: bool) {
        if damaged {
            self.0 |= 1 << 3;
        } else {
            self.0 &= !(1 << 3);
        }
    }

    /// Cities can only be taxed once per turn (p12).
    #[inline]
    pub fn taxed_this_turn(self) -> bool {
        self.0 & (1 << 4) != 0
    }

    #[inline]
    pub fn set_taxed_this_turn(&mut self) {
        self.0 |= 1 << 4;
    }

    /// Starports can only build one ship per turn (p12).
    #[inline]
    pub fn built_this_turn(self) -> bool {
        self.0 & (1 << 5) != 0
    }

    #[inline]
    pub fn set_built_this_turn(&mut self) {
        self.0 |= 1 << 5;
    }

    /// Reset the per-turn flags when a turn finishes.
    #[inline]
    pub fn clear_turn_flags(&mut self) {
        self.0 &= !(0b11 << 4);
    }
}

/// A set of [`ActionKind`]s as a 7-bit mask — the TS `ActionKind[]` lists in
/// `TurnState` have set semantics everywhere they are read.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(transparent))]
pub struct ActionKindSet(pub u8);

impl ActionKindSet {
    pub const EMPTY: ActionKindSet = ActionKindSet(0);

    #[inline]
    pub fn from_kinds(kinds: &[ActionKind]) -> Self {
        let mut set = Self::EMPTY;
        for &k in kinds {
            set.insert(k);
        }
        set
    }

    #[inline]
    pub fn contains(self, kind: ActionKind) -> bool {
        self.0 & (1 << kind.as_index()) != 0
    }

    #[inline]
    pub fn insert(&mut self, kind: ActionKind) {
        self.0 |= 1 << kind.as_index();
    }

    /// Number of kinds in the set — the TS `grant.length` that picks the
    /// most-restrictive free grant.
    #[inline]
    pub fn len(self) -> u32 {
        self.0.count_ones()
    }

    #[inline]
    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    pub fn iter(self) -> impl Iterator<Item = ActionKind> {
        ActionKind::ALL
            .into_iter()
            .filter(move |k| self.contains(*k))
    }
}

// ---------------------------------------------------------------------------
// State structs
// ---------------------------------------------------------------------------

pub const MAX_SEATS: usize = MAX_PLAYERS as usize;

/// One system's mutable state: ship counts per player, buildings, liveness.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SystemState {
    /// Fresh ship count per player.
    pub fresh: [u8; MAX_SEATS],
    /// Damaged ship count per player.
    pub damaged: [u8; MAX_SEATS],
    pub buildings: InlineVec<Building, 2>,
    pub out_of_play: bool,
}

index_enum! {
    /// A destroyed Rival piece, or a captured agent moved to Trophies
    /// (p16, p20).
    pub enum TrophyKind {
        Ship,
        Starport,
        City,
        Agent,
    }
}

/// Per-player state. Trophies and captives are count matrices — see the
/// module docs for why that is safe.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PlayerState {
    pub power: u8,
    /// Resource slots. `None` is an open but empty slot; slots beyond
    /// `open_resource_slots` are covered by unbuilt cities and unusable.
    pub resources: [Option<ResourceType>; MAX_RESOURCE_SLOTS],
    /// Outrage marker per resource type: cannot be spent for its Prelude
    /// action.
    pub outrage: ByResource<bool>,
    /// Court card ids held (Guild cards only — Vox cards discard on secure).
    pub guild_cards: InlineVec<CourtCardId, 25>,
    /// Destroyed Rival pieces by owner and kind: counts toward Warlord, and
    /// the pieces go home when Warlord scores (p19).
    pub trophies: [[u8; TrophyKind::COUNT]; MAX_SEATS],
    /// Captured Rival agents, by owner: counts toward Tyrant.
    pub captives: [u8; MAX_SEATS],
    pub agents_supply: u8,
    pub ships_supply: u8,
    pub starports_supply: u8,
    /// Cities placed on the map or destroyed — how far the board is
    /// uncovered.
    pub cities_used: u8,
    pub hand: InlineVec<ActionCardId, 12>,
    /// Tokens gained but not yet placed, oldest first (p17 slot choice).
    ///
    /// Always empty unless [`VariantDef::choose_placement`] is on, and always
    /// drained before any other decision — see `get_pending`.
    pub pending_resources: InlineVec<ResourceType, MAX_RESOURCE_SLOTS>,
    /// Leaders & Lore: this seat's leader. Always `None` in the base game.
    pub leader: Option<LeaderId>,
    /// Leaders & Lore: held lore cards. Always empty in the base game.
    pub lore: InlineVec<LoreId, 4>,
}

impl PlayerState {
    /// The TS `newPlayerState`: full supplies, empty board.
    pub fn new() -> Self {
        PlayerState {
            power: 0,
            resources: [None; MAX_RESOURCE_SLOTS],
            outrage: [false; ResourceType::COUNT],
            guild_cards: InlineVec::new(),
            trophies: [[0; TrophyKind::COUNT]; MAX_SEATS],
            captives: [0; MAX_SEATS],
            agents_supply: AGENTS,
            ships_supply: SHIPS,
            starports_supply: STARPORTS,
            cities_used: 0,
            hand: InlineVec::new(),
            pending_resources: InlineVec::new(),
            leader: None,
            lore: InlineVec::new(),
        }
    }

    /// Resource slots currently open for this player.
    #[inline]
    pub fn open_resource_slots(&self) -> usize {
        open_resource_slots(self.cities_used)
    }

    /// Put a resource in the leftmost empty open slot. Returns false when
    /// the player has no room — "you must discard resources you cannot
    /// hold" (p17). (`gainResource` in playerBoard.ts.)
    pub fn gain_resource(&mut self, r: ResourceType) -> bool {
        match self.free_slot() {
            Some(slot) => {
                self.resources[slot] = Some(r);
                true
            }
            None => false,
        }
    }

    /// The leftmost free open slot, if any.
    fn free_slot(&self) -> Option<usize> {
        let open = self.open_resource_slots();
        (0..open).find(|&i| self.resources[i].is_none())
    }

    /// Take a resource, deferring *which* slot it lands in to the holder
    /// (p17: "when you gain a resource you may rearrange slots").
    ///
    /// The token is queued rather than placed, so a caller's control flow is
    /// unchanged — the answer to "is there room?" does not depend on which
    /// slot is chosen, only on how many are free. `false` still means the
    /// token cannot be held and must be discarded.
    pub fn queue_resource(&mut self, r: ResourceType) -> bool {
        if self.free_slots() == 0 {
            return false;
        }
        self.pending_resources.push(r);
        true
    }

    /// Open slots that are neither occupied nor already spoken for by a
    /// queued token.
    pub fn free_slots(&self) -> usize {
        let open = self.open_resource_slots();
        let occupied = self.resources.iter().take(open).flatten().count();
        open - occupied - self.pending_resources.len().min(open - occupied)
    }

    /// The distinct raid-cost tiers with a free slot — the placement options
    /// for the next queued token.
    ///
    /// Tiers, not slots: [`crate::player_board::RAID_COSTS`] is `[1,1,2,2,3,3]`,
    /// and the slot index has no effect on anything except its raid cost, so
    /// two free slots of equal cost are the same choice. That quotient is what
    /// keeps the branching factor at most 3 instead of 6 (and far below the
    /// n² of a swap encoding or the n! of a permutation one).
    pub fn placement_tiers(&self) -> InlineVec<u8, 3> {
        let open = self.open_resource_slots();
        let mut out = InlineVec::new();
        for i in 0..open {
            if self.resources[i].is_none() {
                let tier = raid_cost(i);
                if !out.contains(&tier) {
                    out.push(tier);
                }
            }
        }
        out
    }

    /// Place the next queued token in the cheapest free slot of `tier`.
    ///
    /// Within a tier the slot is arbitrary — every free slot of the same cost
    /// is indistinguishable — so the lowest index is the canonical
    /// representative.
    /// Returns whether it placed: `false` when nothing is queued, or when the
    /// tier has no free slot.
    #[must_use]
    pub fn place_queued(&mut self, tier: u8) -> bool {
        if self.pending_resources.is_empty() {
            return false;
        }
        let open = self.open_resource_slots();
        let slot = (0..open).find(|&i| self.resources[i].is_none() && raid_cost(i) == tier);
        let Some(slot) = slot else { return false };
        let r = self.pending_resources.remove(0);
        self.resources[slot] = Some(r);
        true
    }

    /// Discard resources sitting in slots that a returning city has covered,
    /// returning the discarded tokens. (`compactResources` in playerBoard.ts
    /// — which silently deletes the surplus tokens; the Rust port hands them
    /// back so the caller can return them to the supply, since "you must
    /// discard resources you cannot hold" (p17) discards *tokens*, and
    /// tokens never leave the game.)
    pub fn compact_resources(&mut self) -> InlineVec<ResourceType, MAX_RESOURCE_SLOTS> {
        let open = self.open_resource_slots();
        let all: InlineVec<ResourceType, MAX_RESOURCE_SLOTS> =
            self.resources.iter().flatten().copied().collect();
        self.resources = [None; MAX_RESOURCE_SLOTS];
        for (i, r) in all.iter().take(open).enumerate() {
            self.resources[i] = Some(*r);
        }
        all.iter().skip(open).copied().collect()
    }

    /// All held resources, in slot order. (`heldResources` in playerBoard.ts.)
    pub fn held_resources(&self) -> InlineVec<ResourceType, MAX_RESOURCE_SLOTS> {
        self.resources.iter().flatten().copied().collect()
    }

    /// Total trophies (the Warlord count).
    pub fn trophy_count(&self) -> u8 {
        self.trophies.iter().flatten().sum()
    }

    /// Total captives (the Tyrant count).
    pub fn captive_count(&self) -> u8 {
        self.captives.iter().sum()
    }
}

impl Default for PlayerState {
    fn default() -> Self {
        Self::new()
    }
}

/// A face-up Court slot and the agents on it.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct CourtSlot {
    pub card: CourtCardId,
    /// Agents on this card, per player.
    pub agents: [u8; MAX_SEATS],
}

/// One card played into the current round's trick.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PlayedCard {
    pub player: Player,
    pub card: ActionCardId,
    pub mode: PlayMode,
    /// Copy plays and seize cards are face down — hidden from other players.
    pub face_down: bool,
}

impl Default for PlayedCard {
    fn default() -> Self {
        PlayedCard {
            player: Player(0),
            card: ActionCardId(0),
            mode: PlayMode::Lead,
            face_down: false,
        }
    }
}

/// The state of the round in progress.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct RoundState {
    /// Index into `turn_order` of the player currently taking a turn.
    pub turn_index: u8,
    /// Player order for this round, starting from the initiative holder.
    pub turn_order: InlineVec<Player, MAX_SEATS>,
    pub lead: Option<PlayedCard>,
    /// Lead card's effective number: 0 once an ambition is declared on it.
    pub lead_number: u8,
    /// At most one card per player plus one seize card per round.
    pub played: InlineVec<PlayedCard, { MAX_SEATS + 2 }>,
    /// Player who seized the initiative this round, if any.
    pub seized_by: Option<Player>,
    /// Consecutive passes, to detect "everyone with cards passed" (p8).
    pub consecutive_passes: u8,
    /// Any ambition declared yet this round — Galactic Bards needs none to
    /// be.
    pub ambition_declared: bool,
}

/// The turn in progress: what the current player has left to spend.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TurnState {
    pub player: Player,
    pub mode: PlayMode,
    /// The action card this turn was played with — Galactic Bards declares
    /// on it.
    pub card: ActionCardId,
    /// Actions still available from the played card's pips.
    pub pips_left: u8,
    /// Which action kinds the remaining pips may buy.
    pub pip_actions: ActionKindSet,
    /// Free actions granted by spent resources, resolved one at a time.
    ///
    /// One grant per non-Weapon token spent, and a Prelude can spend more
    /// than a slotful: `fillSlots` (the Interest cards) refills empty slots
    /// mid-Prelude, so the player spends, refills and spends again. The hard
    /// bound is the 25 tokens in the box, since a spent token sits off-board
    /// until the Prelude ends and so cannot be spent twice.
    pub free_actions: InlineVec<ActionKindSet, 32>,
    /// A Weapon spent in the Prelude adds Battle to this turn's pip actions.
    pub weapon_spent: bool,
    /// Prelude is over once the first pip is spent or actions have begun.
    pub prelude_over: bool,
    /// Ambition already declared this turn — cannot declare twice on one
    /// card.
    pub declared_this_turn: bool,
    /// Resources spent in the Prelude, returned to the supply when it ends.
    ///
    /// Counts per type rather than a list: only the tally is ever read, and a
    /// count array cannot overflow the way a bounded list can when
    /// `fillSlots` refills slots mid-Prelude (at most 5 tokens of a type
    /// exist).
    pub prelude_spent: [u8; ResourceType::COUNT],
    /// Court cards secured this turn: their Preludes are unusable (p20).
    /// Bounded by the Court deck, since a card can only be secured once.
    pub secured_this_prelude: InlineVec<CourtCardId, 32>,
    /// Guild cards whose once-per-turn Prelude has already been used.
    pub card_preludes_used: InlineVec<CourtCardId, 4>,
}

/// A battle mid-resolution.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct BattleState {
    /// The faces showing on the table, as indices into `DIE_FACES[type]`.
    pub rolled: ByDieType<InlineVec<u8, DICE_PER_TYPE>>,
    pub system: SystemId,
    pub attacker: Player,
    pub defender: Player,
    pub dice: ByDieType<u8>,
    /// Rolled totals, consumed as they are assigned.
    pub self_hits: u8,
    pub intercept: u8,
    pub hits: u8,
    pub building_hits: u8,
    pub keys: u8,
    /// Set once the intercept has been converted into self-hits.
    pub intercept_resolved: bool,
    /// Skirmish dice that came up blank — the ones Skirmishers may reroll.
    pub skirmish_blanks: u8,
    /// Dice the next battle roll should reroll instead of rolling the pool.
    pub pending_reroll: u8,
    /// Skirmishers is once per battle.
    pub reroll_done: bool,
}

/// A catapult move in progress (p13).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct MoveState {
    /// Where the travelling ships are.
    pub at: SystemId,
    pub ships: u8,
    /// Systems this catapult has already passed through (engine ruling: no
    /// revisits, see game.ts).
    pub visited: InlineVec<SystemId, SYSTEM_COUNT>,
}

/// A Union card attached to a face-up played action card (p20).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct UnionAttachment {
    pub card: CourtCardId,
    pub player: Player,
    pub target: ActionCardId,
}

/// What a pending Vox effect resumes into once answered.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum VoxResume {
    Actions,
    Battle,
}

/// A Vox card's `When Secured` effect waiting to be resolved. It overlays
/// the phase rather than replacing it — see game.ts.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PendingVox {
    pub card: CourtCardId,
    pub player: Player,
    pub resume: VoxResume,
}

/// Farseers: whose hand the player is looking at, and where to return to.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Peek {
    pub player: Player,
    pub target: Option<Player>,
    pub resume: Phase,
}

/// A follower choosing not to Surpass — evidence about their hand.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Decline {
    pub player: Player,
    pub suit: Suit,
    pub number: u8,
}

impl Default for Decline {
    fn default() -> Self {
        Decline {
            player: Player(0),
            suit: Suit::Administration,
            number: 0,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct GameStats {
    pub rounds: u16,
    pub chapters: u16,
    pub battles: u16,
    pub cards_played: u16,
    pub ambitions_declared: u16,
    pub seizes: u16,
}

/// The complete mutable game state — flat, `Copy`, at most 4 KB.
///
/// Seats beyond `players` hold default (untouched) values everywhere a
/// `MAX_SEATS`-sized array appears.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct GameState {
    pub players: u8,
    pub chapter: u8,
    pub phase: Phase,
    /// Holder of the initiative marker.
    pub initiative: Player,
    /// True once someone has seized it this round (it lies down).
    pub initiative_seized: bool,
    pub systems: [SystemState; SYSTEM_COUNT],
    pub player_states: [PlayerState; MAX_SEATS],
    /// The general supply: 5 tokens of each resource type (p3).
    pub supply: ByResource<u8>,
    /// Resources sitting on a Cartel card instead of in the general supply,
    /// keyed by type — there is exactly one Cartel per type in the deck.
    pub cartel: ByResource<u8>,
    pub court: InlineVec<CourtSlot, 4>,
    pub court_deck: InlineVec<CourtCardId, COURT_CARD_COUNT>,
    pub court_discard: InlineVec<CourtCardId, COURT_CARD_COUNT>,
    pub action_deck: InlineVec<ActionCardId, ACTION_CARD_COUNT>,
    pub action_discard: InlineVec<ActionCardId, ACTION_CARD_COUNT>,
    pub round: RoundState,
    pub turn: Option<TurnState>,
    pub battle: Option<BattleState>,
    /// The TS `move` field (`move` is a Rust keyword).
    pub moving: Option<MoveState>,
    /// Ambition boxes -> indices into `variant.ambition_markers`.
    pub declared: [InlineVec<u8, AMBITION_MARKER_COUNT>; AmbitionId::COUNT],
    /// Markers still in the Available Markers area.
    pub available_markers: InlineVec<u8, AMBITION_MARKER_COUNT>,
    /// Markers flipped to their orange side (by marker index).
    pub flipped: [bool; AMBITION_MARKER_COUNT],
    /// 2-player only: phantom resources on ambition boxes (p19).
    pub phantom: [u8; AmbitionId::COUNT],
    /// Player still owing a reinforce decision at end of turn.
    pub reinforcing: Option<Player>,
    /// Union cards attached to face-up played action cards (p20).
    pub unions: InlineVec<UnionAttachment, 4>,
    pub pending_vox: Option<PendingVox>,
    pub peek: Option<Peek>,
    /// Public memory, reset each chapter: action cards played face up and
    /// since discarded — they cannot be in anyone's hand.
    pub revealed: InlineVec<ActionCardId, ACTION_CARD_COUNT>,
    /// A follower declining to Surpass, public evidence for belief models.
    /// Capacity-bounded; a dropped entry only loses belief information,
    /// never rules state.
    pub declines: InlineVec<Decline, 32>,
    pub stats: GameStats,
}

// ---------------------------------------------------------------------------
// Board queries (mirroring src/engine/board.ts)
// ---------------------------------------------------------------------------

impl GameState {
    #[inline]
    pub fn player(&self, p: Player) -> &PlayerState {
        &self.player_states[p.as_index()]
    }

    #[inline]
    pub fn player_mut(&mut self, p: Player) -> &mut PlayerState {
        &mut self.player_states[p.as_index()]
    }

    /// "You control a system and its contents if you have more fresh ships
    /// there than each Rival. On a tie, no one controls the system." (p7)
    pub fn control_of(&self, system: SystemId) -> Option<Player> {
        let fresh = &self.systems[system.as_index()].fresh;
        let mut best = 0u8;
        let mut winner = None;
        let mut tied = false;
        for (p, &n) in fresh.iter().enumerate().take(self.players as usize) {
            match n.cmp(&best) {
                core::cmp::Ordering::Greater => {
                    best = n;
                    winner = Some(Player(p as u8));
                    tied = false;
                }
                core::cmp::Ordering::Equal => tied = true,
                core::cmp::Ordering::Less => {}
            }
        }
        if best == 0 || tied { None } else { winner }
    }

    #[inline]
    pub fn ships_of(&self, system: SystemId, player: Player) -> u8 {
        let st = &self.systems[system.as_index()];
        st.fresh[player.as_index()] + st.damaged[player.as_index()]
    }

    /// Does the player have any piece (ship or building) in the system?
    pub fn has_piece(&self, system: SystemId, player: Player) -> bool {
        if self.ships_of(system, player) > 0 {
            return true;
        }
        self.systems[system.as_index()]
            .buildings
            .iter()
            .any(|b| b.player() == player)
    }

    pub fn has_building(
        &self,
        system: SystemId,
        player: Player,
        kind: Option<BuildingKind>,
    ) -> bool {
        self.systems[system.as_index()]
            .buildings
            .iter()
            .any(|b| b.player() == player && kind.is_none_or(|k| b.kind() == k))
    }

    /// No ships and no starports anywhere on the map (p22, elimination).
    pub fn is_wiped_out(&self, player: Player) -> bool {
        for i in 0..SYSTEM_COUNT {
            let system = SystemId(i as u8);
            if self.systems[i].out_of_play {
                continue;
            }
            if self.ships_of(system, player) > 0 {
                return false;
            }
            if self.has_building(system, player, Some(BuildingKind::Starport)) {
                return false;
            }
        }
        true
    }

    /// Take a resource from the general supply into a player's slots.
    pub fn take_from_supply(&mut self, v: &VariantDef, player: Player, r: ResourceType) -> bool {
        if self.supply[r.as_index()] == 0 {
            return false;
        }
        if !self.gain_into(v, player, r) {
            return false;
        }
        self.supply[r.as_index()] -= 1;
        true
    }

    /// Give a token to a player. Queued for the holder to place when the
    /// variant models the p17 slot choice, filled leftmost otherwise.
    ///
    /// Either way `false` means "no room" — the answer does not depend on
    /// *which* slot is chosen, so every caller's control flow is identical
    /// under both settings.
    pub fn gain_into(&mut self, v: &VariantDef, player: Player, r: ResourceType) -> bool {
        if v.choose_placement {
            self.player_mut(player).queue_resource(r)
        } else {
            self.player_mut(player).gain_resource(r)
        }
    }

    /// The seat owing a placement decision, if any.
    pub fn placement_owner(&self) -> Option<Player> {
        (0..self.players)
            .map(Player)
            .find(|&p| !self.player(p).pending_resources.is_empty())
    }

    /// Return a resource token to the general supply — or onto the Cartel
    /// card for its type, while one is in play: the card is where that
    /// type's supply lives (p20). (`returnToSupply` in board.ts.)
    pub fn return_to_supply(&mut self, r: ResourceType) {
        if crate::powers::cartel_holder(self, r).is_some() {
            self.cartel[r.as_index()] += 1;
        } else {
            self.supply[r.as_index()] += 1;
        }
    }

    /// Compact a player's resources after a city returns to their board,
    /// sending the surplus tokens back to the supply.
    pub fn compact_resources_of(&mut self, player: Player) {
        let dropped = self.player_mut(player).compact_resources();
        for &r in dropped.iter() {
            self.return_to_supply(r);
        }
    }

    /// All in-play systems adjacent to `system`.
    pub fn neighbours(&self, v: &VariantDef, system: SystemId) -> InlineVec<SystemId, 6> {
        // Out-of-play systems already have no edges after `resolve_adjacency`,
        // but keep the TS guard for state constructed by hand.
        v.systems[system.as_index()]
            .adjacent
            .iter()
            .copied()
            .filter(|s| !self.systems[s.as_index()].out_of_play)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn game_state_is_flat_and_small() {
        // The plan's budget: a Copy state at most 4 KB, so clone is memcpy
        // and search states live on the stack.
        assert!(
            size_of::<GameState>() <= 4096,
            "GameState is {} bytes",
            size_of::<GameState>()
        );
    }

    #[test]
    fn building_packs_and_unpacks() {
        for player in 0..4u8 {
            for kind in BuildingKind::ALL {
                for damaged in [false, true] {
                    let mut b = Building::new(Player(player), kind, damaged);
                    assert_eq!(b.player(), Player(player));
                    assert_eq!(b.kind(), kind);
                    assert_eq!(b.damaged(), damaged);
                    assert!(!b.taxed_this_turn());
                    assert!(!b.built_this_turn());
                    b.set_taxed_this_turn();
                    b.set_built_this_turn();
                    assert!(b.taxed_this_turn() && b.built_this_turn());
                    b.clear_turn_flags();
                    assert!(!b.taxed_this_turn() && !b.built_this_turn());
                    assert_eq!(b.kind(), kind);
                    b.set_damaged(!damaged);
                    assert_eq!(b.damaged(), !damaged);
                }
            }
        }
    }

    #[test]
    fn action_kind_set_behaves_like_a_set() {
        use ActionKind::*;
        let set = ActionKindSet::from_kinds(&[Tax, Repair, Influence]);
        assert!(set.contains(Tax) && set.contains(Repair) && set.contains(Influence));
        assert!(!set.contains(Battle));
        assert_eq!(set.len(), 3);
        assert_eq!(set.iter().collect::<Vec<_>>(), vec![Tax, Repair, Influence]);
        assert!(ActionKindSet::EMPTY.is_empty());
    }

    // Ported from tests/rules.test.ts "opens resource slots as cities leave
    // the player board (p7)" — via the PlayerState wrapper.
    #[test]
    fn resource_slots_open_as_cities_leave() {
        let mut p = PlayerState::new();
        let before = p.open_resource_slots();
        p.cities_used = 5;
        assert!(p.open_resource_slots() > before);
    }

    #[test]
    fn gain_and_compact_resources() {
        let mut p = PlayerState::new();
        assert_eq!(p.open_resource_slots(), 3);
        assert!(p.gain_resource(ResourceType::Fuel));
        assert!(p.gain_resource(ResourceType::Relic));
        assert!(p.gain_resource(ResourceType::Fuel));
        // All 3 open slots full.
        assert!(!p.gain_resource(ResourceType::Material));
        assert_eq!(p.held_resources().len(), 3);

        // A returning city covers a slot: the surplus resource is discarded.
        p.cities_used = 1; // 4 open slots
        assert!(p.gain_resource(ResourceType::Psionic));
        p.cities_used = 0;
        p.compact_resources();
        assert_eq!(p.held_resources().len(), 3);
        assert_eq!(p.resources[3], None);
    }

    #[test]
    fn control_needs_strictly_most_fresh_ships() {
        let mut s = GameState {
            players: 3,
            chapter: 1,
            phase: Phase::Play,
            initiative: Player(0),
            initiative_seized: false,
            systems: [SystemState::default(); SYSTEM_COUNT],
            player_states: [PlayerState::new(); MAX_SEATS],
            supply: [5; ResourceType::COUNT],
            cartel: [0; ResourceType::COUNT],
            court: InlineVec::new(),
            court_deck: InlineVec::new(),
            court_discard: InlineVec::new(),
            action_deck: InlineVec::new(),
            action_discard: InlineVec::new(),
            round: RoundState::default(),
            turn: None,
            battle: None,
            moving: None,
            declared: Default::default(),
            available_markers: InlineVec::new(),
            flipped: [false; AMBITION_MARKER_COUNT],
            phantom: [0; AmbitionId::COUNT],
            reinforcing: None,
            unions: InlineVec::new(),
            pending_vox: None,
            peek: None,
            revealed: InlineVec::new(),
            declines: InlineVec::new(),
            stats: GameStats::default(),
        };
        let sys = SystemId(1);
        assert_eq!(s.control_of(sys), None); // empty
        s.systems[1].fresh[0] = 2;
        s.systems[1].fresh[1] = 2;
        assert_eq!(s.control_of(sys), None); // tied
        s.systems[1].fresh[0] = 3;
        assert_eq!(s.control_of(sys), Some(Player(0)));
        // Damaged ships do not confer control.
        s.systems[1].damaged[1] = 9;
        assert_eq!(s.control_of(sys), Some(Player(0)));
    }
}
