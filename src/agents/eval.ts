/**
 * The heuristic evaluation function — the main experimentation surface.
 *
 * It scores a position from one player's point of view in "Power-equivalent"
 * units: banked Power, plus what the declared ambition boxes are currently
 * worth to them, plus the latent value of an economy and a fleet that have not
 * been cashed in yet.
 *
 * Every weight is a parameter; pass your own via `makeGreedy({ weights })`.
 */
import {
  ambitionCount,
  AMBITIONS,
  controlOf,
  courtCard,
  markerValue,
  openResourceSlots,
  uncoveredBonuses,
} from '../engine';
import type { AmbitionId, GameState, VariantDef } from '../engine';

export interface Weights {
  /** Power already banked. The unit everything else is measured against. */
  power: number;
  /** Weight on ambition boxes the player is currently winning or placing in. */
  declaredLead: number;
  /** Weight on being close to first place in a declared box. */
  declaredContest: number;
  /**
   * Value of an ambition-relevant token held while nothing is declared: it can
   * still be cashed in a later chapter.
   */
  latentAmbition: number;
  /** A fresh ship on the map. */
  freshShip: number;
  /** A damaged ship — still counts for presence, not for control. */
  damagedShip: number;
  /** A starport: builds ships and enables Catapult moves. */
  starport: number;
  /** A city: the tax engine, and it uncovers player-board rewards. */
  city: number;
  /** Systems the player controls outright. */
  control: number;
  /** An open resource slot the player can actually fill. */
  resourceSlot: number;
  /** A held resource token, before ambition value. */
  resource: number;
  /** An agent sitting in the Court. */
  courtAgent: number;
  /** Being the sole leader on a Court card — one Secure away. */
  courtLead: number;
  /** A Guild card held. */
  guildCard: number;
  /** Holding the initiative marker. */
  initiative: number;
  /** Cards left in hand: options for the rest of the chapter. */
  handCard: number;
  /** Penalty per Outraged resource type. */
  outrage: number;
}

export const defaultWeights: Weights = {
  power: 1,
  declaredLead: 0.9,
  declaredContest: 0.35,
  latentAmbition: 0.45,
  freshShip: 0.5,
  damagedShip: 0.2,
  starport: 1.4,
  city: 2.2,
  control: 0.5,
  resourceSlot: 0.4,
  resource: 0.7,
  courtAgent: 0.35,
  courtLead: 1.1,
  guildCard: 1.0,
  initiative: 1.2,
  handCard: 0.25,
  outrage: 1.0,
};

/** Power a player would score right now if the chapter ended. */
export function projectedAmbitionPower(
  s: GameState,
  v: VariantDef,
  player: number,
): { locked: number; contested: number } {
  let locked = 0;
  let contested = 0;

  for (const ambition of AMBITIONS) {
    const markers = s.declared[ambition];
    if (markers.length === 0) continue;

    let first = 0;
    let second = 0;
    for (const i of markers) {
      const value = markerValue(v.ambitionMarkers, i, s.flipped[i]);
      first += value.first;
      second += value.second;
    }

    const mine = ambitionCount(s.playerStates[player], ambition);
    if (mine === 0) continue;

    const others = s.playerStates
      .map((p, i) => (i === player ? -1 : ambitionCount(p, ambition)))
      .filter((c) => c >= 0);
    const phantom = s.phantom[ambition] ?? 0;
    const best = Math.max(0, ...others, phantom);

    if (mine > best) {
      locked += first;
      const { plusTwo, plusThree } = uncoveredBonuses(s.playerStates[player]);
      locked += plusTwo && plusThree ? 5 : plusTwo ? 2 : 0;
    } else if (mine === best) {
      locked += second;
    } else {
      // Behind: worth something in proportion to how close the gap is.
      contested += second * (mine / (best || 1));
    }
  }
  return { locked, contested };
}

/** Ambition-relevant tokens held while no box is declared for them. */
function latentAmbitionValue(s: GameState, player: number): number {
  let n = 0;
  for (const ambition of AMBITIONS) {
    if (s.declared[ambition].length > 0) continue;
    n += ambitionCount(s.playerStates[player], ambition as AmbitionId);
  }
  return n;
}

export function evaluate(
  s: GameState,
  v: VariantDef,
  player: number,
  w: Weights = defaultWeights,
): number {
  const p = s.playerStates[player];
  let value = p.power * w.power;

  const { locked, contested } = projectedAmbitionPower(s, v, player);
  value += locked * w.declaredLead + contested * w.declaredContest;
  value += latentAmbitionValue(s, player) * w.latentAmbition;

  for (let i = 0; i < s.systems.length; i++) {
    const st = s.systems[i];
    if (st.outOfPlay) continue;
    value += st.fresh[player] * w.freshShip;
    value += st.damaged[player] * w.damagedShip;
    for (const b of st.buildings) {
      if (b.player !== player) continue;
      const base = b.kind === 'city' ? w.city : w.starport;
      value += b.damaged ? base * 0.5 : base;
    }
    if (controlOf(s, i) === player) value += w.control;
  }

  const open = openResourceSlots(p);
  value += open * w.resourceSlot;
  for (let i = 0; i < open; i++) if (p.resources[i]) value += w.resource;
  for (const g of p.guildCards) value += w.guildCard + (courtCard(g).suit ? 0 : 0);

  for (const slot of s.court) {
    const mine = slot.agents[player];
    if (mine === 0) continue;
    value += mine * w.courtAgent;
    const rivalBest = Math.max(
      0,
      ...slot.agents.map((n, i) => (i === player ? 0 : n)),
    );
    if (mine > rivalBest) value += w.courtLead;
  }

  if (s.initiative === player) value += w.initiative;
  value += p.hand.length * w.handCard;
  for (const r of Object.keys(p.outrage) as (keyof typeof p.outrage)[]) {
    if (p.outrage[r]) value -= w.outrage;
  }

  return value;
}

/**
 * Evaluation relative to the field: how far ahead of the best Rival the player
 * is. Multiplayer search wants this rather than raw self-value, so a bot does
 * not happily hand the lead to someone else.
 */
export function relativeEvaluate(
  s: GameState,
  v: VariantDef,
  player: number,
  w: Weights = defaultWeights,
): number {
  const mine = evaluate(s, v, player, w);
  let best = -Infinity;
  for (let p = 0; p < s.players; p++) {
    if (p === player) continue;
    best = Math.max(best, evaluate(s, v, p, w));
  }
  return mine - (best === -Infinity ? 0 : best);
}
