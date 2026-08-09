//! The heuristic evaluation function — the main experimentation surface.
//! Ported from `src/agents/eval.ts` (the source of truth during the port).
//!
//! It scores a position from one player's point of view in "Power-equivalent"
//! units: banked Power, plus what the declared ambition boxes are currently
//! worth to them, plus the latent value of an economy and a fleet that have
//! not been cashed in yet.
//!
//! Every weight is a parameter; pass your own through [`crate::AgentOpts`].
//!
//! The arithmetic is plain `f64` in the TS statement order, so a Rust
//! evaluation of a state reproduces the TS number bit for bit whenever the
//! two engines are handed the same position. Seats past `s.players` are
//! skipped everywhere the TS code iterates a `players`-long array — the Rust
//! per-seat arrays are always `MAX_SEATS` wide and the dead seats hold
//! defaults.

use arcs_engine::ambitions::{ambition_count, marker_value};
use arcs_engine::cards::action_card;
use arcs_engine::map::SYSTEM_COUNT;
use arcs_engine::player_board::{open_resource_slots, uncovered_bonuses};
use arcs_engine::types::ByResource;
use arcs_engine::{
    AmbitionId, BuildingKind, GameState, Player, ResourceType, Suit, SystemId, VariantDef,
};

/// Weights for [`evaluate`]. Every field is one term of the linear
/// evaluation; see the TS doc comments for what each one is buying.
#[derive(Clone, Copy, PartialEq, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Weights {
    /// Power already banked. The unit everything else is measured against.
    pub power: f64,
    /// Weight on ambition boxes the player is currently winning or placing
    /// in.
    pub declared_lead: f64,
    /// Weight on being close to first place in a declared box.
    pub declared_contest: f64,
    /// Value of an ambition-relevant token held while nothing is declared: it
    /// can still be cashed in a later chapter.
    pub latent_ambition: f64,
    /// A fresh ship on the map.
    pub fresh_ship: f64,
    /// A damaged ship — still counts for presence, not for control.
    pub damaged_ship: f64,
    /// A starport: builds ships and enables Catapult moves.
    pub starport: f64,
    /// A city: the tax engine, and it uncovers player-board rewards.
    pub city: f64,
    /// Systems the player controls outright.
    pub control: f64,
    /// An open resource slot the player can actually fill.
    pub resource_slot: f64,
    /// A held resource token, before ambition value, priced per type. The map
    /// prints Material and Fuel on 4 planets each but Relic and Psionic on 3,
    /// and Weapon — the commonest to raid — scores no ambition at all, so a
    /// flat price misranks every trade.
    pub resource_value: ByResource<f64>,
    /// An agent sitting in the Court.
    pub court_agent: f64,
    /// Being the sole leader on a Court card — one Secure away.
    pub court_lead: f64,
    /// A Guild card held.
    pub guild_card: f64,
    /// Holding the initiative marker.
    pub initiative: f64,
    /// Cards left in hand: options for the rest of the chapter.
    pub hand_card: f64,
    /// Total pips across the hand. A hand of 2s can act four times per lead;
    /// a hand of 6s, twice. Counting cards alone made Farseers' recycle —
    /// swap n cards for n+1 — read as a pure Guild-card loss, so it was never
    /// taken.
    pub hand_pips: f64,
    /// A hand card whose suit can actually act on the current position.
    pub hand_actionable: f64,
    /// Holding the highest card of a suit still in any hand: nobody can
    /// Surpass it, so it carries the initiative whenever it leads.
    pub hand_high_card: f64,
    /// Strictly leading an undeclared ambition while markers remain to
    /// declare it. The lead is only worth something while it can still be
    /// cashed in.
    pub declarable_lead: f64,
    /// Penalty per Outraged resource type.
    pub outrage: f64,
}

/// The live default weights (`defaultWeights` in eval.ts). Tuning these is
/// the experimentation surface; the frozen [`crate::anchors`] never move.
pub const DEFAULT_WEIGHTS: Weights = Weights {
    power: 1.0,
    declared_lead: 0.9,
    declared_contest: 0.35,
    latent_ambition: 0.35,
    fresh_ship: 0.5,
    damaged_ship: 0.2,
    starport: 1.4,
    city: 2.2,
    control: 0.5,
    resource_slot: 0.4,
    resource_value: [0.65, 0.65, 0.5, 0.85, 0.85],
    court_agent: 0.35,
    court_lead: 1.1,
    guild_card: 1.0,
    initiative: 1.2,
    hand_card: 0.1,
    hand_pips: 0.15,
    hand_actionable: 0.1,
    hand_high_card: 0.3,
    declarable_lead: 0.5,
    outrage: 1.0,
};

/// [`DEFAULT_WEIGHTS`], as a function for parity with the TS export.
#[inline]
pub fn default_weights() -> Weights {
    DEFAULT_WEIGHTS
}

impl Default for Weights {
    fn default() -> Self {
        DEFAULT_WEIGHTS
    }
}

/// Power a player would score right now if the chapter ended.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct ProjectedAmbition {
    /// Power the player would bank outright.
    pub locked: f64,
    /// Second-place Power discounted by how far behind the leader they are.
    pub contested: f64,
}

/// (`projectedAmbitionPower` in eval.ts.)
pub fn projected_ambition_power(
    s: &GameState,
    v: &VariantDef,
    player: Player,
) -> ProjectedAmbition {
    let mut locked = 0.0f64;
    let mut contested = 0.0f64;

    for ambition in AmbitionId::ALL {
        let markers = &s.declared[ambition.as_index()];
        if markers.is_empty() {
            continue;
        }

        let mut first = 0.0f64;
        let mut second = 0.0f64;
        for &i in markers.iter() {
            let value = marker_value(&v.ambition_markers, i as usize, s.flipped[i as usize]);
            first += value.first as f64;
            second += value.second as f64;
        }

        let mine = ambition_count(s.player(player), ambition) as f64;
        if mine == 0.0 {
            continue;
        }

        let mut best = s.phantom[ambition.as_index()] as f64;
        for p in (0..s.players).map(Player) {
            if p == player {
                continue;
            }
            best = best.max(ambition_count(s.player(p), ambition) as f64);
        }

        if mine > best {
            locked += first;
            let (plus_two, plus_three) = uncovered_bonuses(s.player(player).cities_used);
            locked += if plus_two && plus_three {
                5.0
            } else if plus_two {
                2.0
            } else {
                0.0
            };
        } else if mine == best {
            locked += second;
        } else {
            // Behind: worth something in proportion to how close the gap is.
            contested += second * (mine / if best == 0.0 { 1.0 } else { best });
        }
    }
    ProjectedAmbition { locked, contested }
}

/// Ambition-relevant tokens held while no box is declared for them.
fn latent_ambition_value(s: &GameState, player: Player) -> f64 {
    let mut n = 0.0f64;
    for ambition in AmbitionId::ALL {
        if !s.declared[ambition.as_index()].is_empty() {
            continue;
        }
        n += ambition_count(s.player(player), ambition) as f64;
    }
    n
}

/// Undeclared ambitions this player strictly leads while a marker remains to
/// declare them with. Distinct from the latent term: tokens are worth a
/// little on their own, but a *lead* that can still be declared is a claim on
/// a whole ambition box.
fn declarable_leads(s: &GameState, v: &VariantDef, player: Player) -> f64 {
    let mut used = 0usize;
    for ambition in AmbitionId::ALL {
        used += s.declared[ambition.as_index()].len();
    }
    if used >= v.ambition_markers.len() {
        return 0.0;
    }

    let mut leads = 0.0f64;
    for ambition in AmbitionId::ALL {
        if !s.declared[ambition.as_index()].is_empty() {
            continue;
        }
        let mine = ambition_count(s.player(player), ambition);
        if mine == 0 {
            continue;
        }
        let mut best = s.phantom[ambition.as_index()];
        for p in (0..s.players).map(Player) {
            if p == player {
                continue;
            }
            best = best.max(ambition_count(s.player(p), ambition));
        }
        if mine > best {
            leads += 1.0;
        }
    }
    leads
}

/// Which suits can act on the player's actual position right now. A card's
/// pips are only options if the board offers something to spend them on: an
/// Aggression card with no reachable fight is just a number.
fn actionable_suits(s: &GameState, v: &VariantDef, player: Player) -> [bool; Suit::COUNT] {
    let mut can_tax = false;
    let mut can_battle = false;
    let mut can_build = false;
    let mut can_move = false;

    for i in 0..SYSTEM_COUNT {
        let st = &s.systems[i];
        if st.out_of_play {
            continue;
        }

        let my_fresh = st.fresh[player.as_index()] > 0;
        if my_fresh {
            can_move = true;
        }

        let mut rival_presence = false;
        let mut rival_city = false;
        let mut my_city = false;
        let mut my_starport = false;
        for p in (0..s.players).map(Player) {
            if p == player {
                continue;
            }
            if st.fresh[p.as_index()] > 0 || st.damaged[p.as_index()] > 0 {
                rival_presence = true;
            }
        }
        for b in st.buildings.iter() {
            if b.player() == player {
                if b.kind() == BuildingKind::City {
                    my_city = true;
                } else {
                    my_starport = true;
                }
            } else {
                rival_presence = true;
                if b.kind() == BuildingKind::City {
                    rival_city = true;
                }
            }
        }

        if my_fresh && rival_presence {
            can_battle = true;
        }
        if my_city {
            can_tax = true;
        }

        if !can_build || !can_tax {
            let control = s.control_of(SystemId(i as u8));
            if control == Some(player) {
                if rival_city {
                    can_tax = true;
                }
                if my_starport || st.buildings.len() < v.systems[i].building_slots as usize {
                    can_build = true;
                }
            }
        }
        if can_tax && can_battle && can_build && can_move {
            break;
        }
    }

    // Indexed by `Suit::as_index()`: administration, aggression,
    // construction, mobilization.
    [can_tax, can_battle, can_build, can_move]
}

/// The highest card number of each suit still sitting in someone's hand.
fn top_card_by_suit(s: &GameState) -> [u8; Suit::COUNT] {
    let mut top = [0u8; Suit::COUNT];
    for p in 0..s.players as usize {
        for &c in s.player_states[p].hand.iter() {
            let def = action_card(c);
            if def.number > top[def.suit.as_index()] {
                top[def.suit.as_index()] = def.number;
            }
        }
    }
    top
}

/// Score the position from `player`'s point of view (`evaluate` in eval.ts).
pub fn evaluate(s: &GameState, v: &VariantDef, player: Player, w: &Weights) -> f64 {
    let p = s.player(player);
    let mut value = p.power as f64 * w.power;

    let projected = projected_ambition_power(s, v, player);
    value += projected.locked * w.declared_lead + projected.contested * w.declared_contest;
    value += latent_ambition_value(s, player) * w.latent_ambition;
    value += declarable_leads(s, v, player) * w.declarable_lead;

    for i in 0..SYSTEM_COUNT {
        let st = &s.systems[i];
        if st.out_of_play {
            continue;
        }
        value += st.fresh[player.as_index()] as f64 * w.fresh_ship;
        value += st.damaged[player.as_index()] as f64 * w.damaged_ship;
        for b in st.buildings.iter() {
            if b.player() != player {
                continue;
            }
            let base = if b.kind() == BuildingKind::City {
                w.city
            } else {
                w.starport
            };
            value += if b.damaged() { base * 0.5 } else { base };
        }
        if s.control_of(SystemId(i as u8)) == Some(player) {
            value += w.control;
        }
    }

    let open = open_resource_slots(p.cities_used);
    value += open as f64 * w.resource_slot;
    for r in p.resources.iter().take(open).flatten() {
        value += w.resource_value[r.as_index()];
    }
    for _ in p.guild_cards.iter() {
        value += w.guild_card;
    }

    for slot in s.court.iter() {
        let mine = slot.agents[player.as_index()];
        if mine == 0 {
            continue;
        }
        value += mine as f64 * w.court_agent;
        let mut rival_best = 0u8;
        for q in 0..s.players as usize {
            if q != player.as_index() {
                rival_best = rival_best.max(slot.agents[q]);
            }
        }
        if mine > rival_best {
            value += w.court_lead;
        }
    }

    if s.initiative == player {
        value += w.initiative;
    }

    // Pips already granted to the live turn count like pips still in hand,
    // otherwise a 1-ply search prefers leading its low-pip cards to hoard
    // stock, forfeiting the very actions the pips exist to buy.
    if let Some(turn) = s.turn
        && turn.player == player
    {
        value += turn.pips_left as f64 * w.hand_pips;
    }

    value += p.hand.len() as f64 * w.hand_card;
    if !p.hand.is_empty() {
        let able = actionable_suits(s, v, player);
        let top = top_card_by_suit(s);
        for &c in p.hand.iter() {
            let def = action_card(c);
            value += def.pips as f64 * w.hand_pips;
            if able[def.suit.as_index()] {
                value += w.hand_actionable;
            }
            if def.number == top[def.suit.as_index()] {
                value += w.hand_high_card;
            }
        }
    }

    // A pending Union attachment is a claim on the played card it sits next
    // to — the owner draws it when the round ends — so it is priced like a
    // card already in hand. Without this the attach ability reads as a pure
    // Guild-card loss, the same stock-without-flow shape as the pip credit.
    for u in s.unions.iter() {
        if u.player != player {
            continue;
        }
        value += w.hand_card + action_card(u.target).pips as f64 * w.hand_pips;
    }

    for r in ResourceType::ALL {
        if p.outrage[r.as_index()] {
            value -= w.outrage;
        }
    }

    value
}

/// Evaluation relative to the field: how far ahead of the best Rival the
/// player is. Multiplayer search wants this rather than raw self-value, so a
/// bot does not happily hand the lead to someone else.
/// (`relativeEvaluate` in eval.ts.)
pub fn relative_evaluate(s: &GameState, v: &VariantDef, player: Player, w: &Weights) -> f64 {
    let mine = evaluate(s, v, player, w);
    let mut best = f64::NEG_INFINITY;
    for p in (0..s.players).map(Player) {
        if p == player {
            continue;
        }
        best = best.max(evaluate(s, v, p, w));
    }
    mine - if best == f64::NEG_INFINITY { 0.0 } else { best }
}
