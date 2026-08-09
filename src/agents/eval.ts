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
  actionCard,
  ambitionCount,
  AMBITIONS,
  controlOf,
  courtCard,
  markerValue,
  openResourceSlots,
  uncoveredBonuses,
} from '../engine';
import type { AmbitionId, GameState, ResourceType, Suit, VariantDef } from '../engine';

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
  /**
   * A held resource token, before ambition value, priced per type. The map
   * prints Material and Fuel on 4 planets each but Relic and Psionic on 3,
   * and Weapon — the commonest to raid — scores no ambition at all, so a
   * flat price misranks every trade.
   */
  resourceValue: Record<ResourceType, number>;
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
  /**
   * Total pips across the hand. A hand of 2s can act four times per lead; a
   * hand of 6s, twice. Counting cards alone made Farseers' recycle — swap n
   * cards for n+1 — read as a pure Guild-card loss, so it was never taken.
   */
  handPips: number;
  /** A hand card whose suit can actually act on the current position. */
  handActionable: number;
  /**
   * Holding the highest card of a suit still in any hand: nobody can Surpass
   * it, so it carries the initiative whenever it leads.
   */
  handHighCard: number;
  /**
   * Strictly leading an undeclared ambition while markers remain to declare
   * it. The lead is only worth something while it can still be cashed in.
   */
  declarableLead: number;
  /** Penalty per Outraged resource type. */
  outrage: number;
}

export const defaultWeights: Weights = {
  power: 1,
  declaredLead: 0.9,
  declaredContest: 0.35,
  latentAmbition: 0.35,
  freshShip: 0.5,
  damagedShip: 0.2,
  starport: 1.4,
  city: 2.2,
  control: 0.5,
  resourceSlot: 0.4,
  resourceValue: { material: 0.65, fuel: 0.65, weapon: 0.5, relic: 0.85, psionic: 0.85 },
  courtAgent: 0.35,
  courtLead: 1.1,
  guildCard: 1.0,
  initiative: 1.2,
  handCard: 0.1,
  handPips: 0.15,
  handActionable: 0.1,
  handHighCard: 0.3,
  declarableLead: 0.5,
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

/**
 * Undeclared ambitions this player strictly leads while a marker remains to
 * declare them with. Distinct from the latent term: tokens are worth a little
 * on their own, but a *lead* that can still be declared is a claim on a whole
 * ambition box.
 */
function declarableLeads(s: GameState, v: VariantDef, player: number): number {
  let used = 0;
  for (const ambition of AMBITIONS) used += s.declared[ambition].length;
  if (used >= v.ambitionMarkers.length) return 0;

  let leads = 0;
  for (const ambition of AMBITIONS) {
    if (s.declared[ambition].length > 0) continue;
    const mine = ambitionCount(s.playerStates[player], ambition);
    if (mine === 0) continue;
    let best = s.phantom[ambition] ?? 0;
    for (let p = 0; p < s.players; p++) {
      if (p === player) continue;
      best = Math.max(best, ambitionCount(s.playerStates[p], ambition));
    }
    if (mine > best) leads++;
  }
  return leads;
}

/**
 * Which suits can act on the player's actual position right now. A card's
 * pips are only options if the board offers something to spend them on: an
 * Aggression card with no reachable fight is just a number.
 */
function actionableSuits(s: GameState, v: VariantDef, player: number): Record<Suit, boolean> {
  let canTax = false;
  let canBattle = false;
  let canBuild = false;
  let canMove = false;

  for (let i = 0; i < s.systems.length; i++) {
    const st = s.systems[i];
    if (st.outOfPlay) continue;

    const myFresh = st.fresh[player] > 0;
    if (myFresh) canMove = true;

    let rivalPresence = false;
    let rivalCity = false;
    let myCity = false;
    let myStarport = false;
    for (let p = 0; p < s.players; p++) {
      if (p === player) continue;
      if (st.fresh[p] > 0 || st.damaged[p] > 0) rivalPresence = true;
    }
    for (const b of st.buildings) {
      if (b.player === player) {
        if (b.kind === 'city') myCity = true;
        else myStarport = true;
      } else {
        rivalPresence = true;
        if (b.kind === 'city') rivalCity = true;
      }
    }

    if (myFresh && rivalPresence) canBattle = true;
    if (myCity) canTax = true;

    if (!canBuild || !canTax) {
      const control = controlOf(s, i);
      if (control === player) {
        if (rivalCity) canTax = true;
        if (myStarport || st.buildings.length < v.systems[i].buildingSlots) canBuild = true;
      }
    }
    if (canTax && canBattle && canBuild && canMove) break;
  }

  return {
    administration: canTax,
    aggression: canBattle,
    construction: canBuild,
    mobilization: canMove,
  };
}

/** The highest card number of each suit still sitting in someone's hand. */
function topCardBySuit(s: GameState): Record<Suit, number> {
  const top: Record<Suit, number> = {
    administration: 0,
    aggression: 0,
    construction: 0,
    mobilization: 0,
  };
  for (const p of s.playerStates) {
    for (const c of p.hand) {
      const def = actionCard(c);
      if (def.number > top[def.suit]) top[def.suit] = def.number;
    }
  }
  return top;
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
  value += declarableLeads(s, v, player) * w.declarableLead;

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
  for (let i = 0; i < open; i++) {
    const r = p.resources[i];
    if (r) value += w.resourceValue[r];
  }
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

  // Pips already granted to the live turn count like pips still in hand,
  // otherwise a 1-ply search prefers leading its low-pip cards to hoard
  // stock, forfeiting the very actions the pips exist to buy.
  if (s.turn && s.turn.player === player) value += s.turn.pipsLeft * w.handPips;

  value += p.hand.length * w.handCard;
  if (p.hand.length > 0) {
    const able = actionableSuits(s, v, player);
    const top = topCardBySuit(s);
    for (const c of p.hand) {
      const def = actionCard(c);
      value += def.pips * w.handPips;
      if (able[def.suit]) value += w.handActionable;
      if (def.number === top[def.suit]) value += w.handHighCard;
    }
  }

  // A pending Union attachment is a claim on the played card it sits next
  // to — the owner draws it when the round ends — so it is priced like a
  // card already in hand. Without this the attach ability reads as a pure
  // Guild-card loss, the same stock-without-flow shape as the pip credit.
  for (const u of s.unions) {
    if (u.player !== player) continue;
    value += w.handCard + actionCard(u.target).pips * w.handPips;
  }

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
