/**
 * Guild and Vox card ability dispatch (rulebook p20).
 *
 * The rulebook sorts card powers into kinds, and this module handles each where
 * the state machine reaches for it:
 *
 *   - **`Prelude:` abilities** — enumerated in the prelude phase, most of them
 *     discarding the card to do something once.
 *   - **New actions** written `Name (Standard):` — offered wherever the
 *     standard action they replace is affordable, and paid for the same way.
 *   - **Passive modifiers** — queried by the engine at the point they apply
 *     (extra battle dice, theft immunity, the zero marker, spending a resource
 *     as another type, rerolling skirmish dice).
 *   - **Vox `When Secured:`** — resolved through `s.pendingVox`, since securing
 *     happens mid-turn or mid-battle and that flow has to resume afterwards.
 *
 * Enumeration and application are deliberately kept next to each other for each
 * ability: every `legal*` list here is the exact set `apply*` accepts, which is
 * what lets the agents assert they never play an unoffered action.
 */
import { actionCard } from './cards';
import { courtCard, COURT_DECK } from './court';
import {
  cartelHolder,
  collectCartelSupply,
  controlOf,
  hasPiece,
  releaseCartelSupply,
  returnToSupply,
  takeFromSupply,
} from './board';
import { clusterOf, CLUSTER_COUNT, isGate } from './map';
import { compactResources, openResourceSlots } from './playerBoard';
import type {
  Action,
  AmbitionId,
  GameState,
  PlayerState,
  PlayMode,
  ResourceType,
  VariantDef,
} from './types';
import { AMBITIONS, RESOURCE_TYPES } from './types';

/** Every Guild card a player holds that carries an engine-readable power. */
export function poweredCards(p: PlayerState) {
  return p.guildCards.map(courtCard).filter((c) => c.power);
}

/** Does this player hold a card with the given passive? */
export function hasPassive(p: PlayerState, kind: string): boolean {
  return poweredCards(p).some((c) => c.power!.passives?.some((x) => x.t === kind));
}

/** The resource types a Loyal card lets this player treat other resources as. */
export function loyalTypes(p: PlayerState): ResourceType[] {
  const out: ResourceType[] = [];
  for (const c of poweredCards(p)) {
    for (const passive of c.power!.passives ?? []) {
      if (passive.t === 'loyal') out.push(passive.as);
    }
  }
  return out;
}

/**
 * "If you Provoke Outrage, keep this card" — a Loyal card survives the Outrage
 * discard of its own suit (p16 vs the Loyal cards' text).
 */
export function survivesOutrage(cardId: number): boolean {
  const c = courtCard(cardId);
  return (c.power?.passives ?? []).some((x) => x.t === 'loyal');
}

/** Extra battle dice from passives, given where the battle is (Gatekeepers). */
export function extraBattleDice(p: PlayerState, system: number): number {
  if (!isGate(system)) return 0;
  let extra = 0;
  for (const c of poweredCards(p)) {
    for (const passive of c.power!.passives ?? []) {
      if (passive.t === 'gateDice') extra += passive.count;
    }
  }
  return extra;
}

/** Sworn Guardians: Rivals cannot steal your resources and other Guild cards. */
export function theftImmune(p: PlayerState): boolean {
  return hasPassive(p, 'theftImmune');
}

/** The one card a raider can still take from a theft-immune player. */
export function theftImmunityCard(p: PlayerState): number | null {
  for (const c of poweredCards(p)) {
    if ((c.power!.passives ?? []).some((x) => x.t === 'theftImmune')) return c.id;
  }
  return null;
}

/**
 * Whether declaring skips the zero marker.
 *
 * Secret Order exempts Keeper and Empath; Galactic Bards exempts the ambition
 * it lets you declare on a Surpass or Pivot.
 */
export function skipsZeroMarker(p: PlayerState, ambition: string, mode: PlayMode): boolean {
  for (const c of poweredCards(p)) {
    for (const passive of c.power!.passives ?? []) {
      if (passive.t !== 'noZeroMarker') continue;
      if (passive.ambitions === 'keeperEmpath' && (ambition === 'keeper' || ambition === 'empath')) {
        return true;
      }
      if (passive.ambitions === 'surpassPivot' && (mode === 'surpass' || mode === 'pivot')) {
        return true;
      }
    }
  }
  return false;
}

/**
 * "Your total Weapon icons from resources and cards" — the size gate on
 * Skirmishers' reroll and on Court Enforcers' Abduct.
 */
export function weaponIcons(p: PlayerState): number {
  let n = p.resources.filter((r) => r === 'weapon').length;
  for (const id of p.guildCards) if (courtCard(id).suit === 'weapon') n++;
  return n;
}

/** Skirmishers: may reroll blank skirmish dice after rolling. */
export function canRerollSkirmish(p: PlayerState): boolean {
  return hasPassive(p, 'rerollSkirmish');
}

/** Farseers: on declaring an ambition, look at a Rival hand and may swap. */
export function canPeek(p: PlayerState): boolean {
  return hasPassive(p, 'peekAndSwap');
}

/** Galactic Bards: may declare on a Surpass or Pivot if nobody has yet. */
export function canDeclareOnFollow(p: PlayerState): boolean {
  for (const c of poweredCards(p)) {
    for (const passive of c.power!.passives ?? []) {
      if (passive.t === 'noZeroMarker' && passive.ambitions === 'surpassPivot') return true;
    }
  }
  return false;
}

/** Resources a player holds on Cartel cards, which count toward Tycoon. */
export function cartelIcons(s: GameState, player: number): Record<ResourceType, number> {
  const out = {} as Record<ResourceType, number>;
  for (const r of RESOURCE_TYPES) {
    out[r] = cartelHolder(s, r) === player ? s.cartel[r] : 0;
  }
  return out;
}

// ---------------------------------------------------------------------------
// Gaining and losing Guild cards
// ---------------------------------------------------------------------------

/**
 * Give a Guild card to a player, collecting a Cartel's supply onto it.
 *
 * Every route into a player's play area goes through here — securing, raiding,
 * Silver-Tongues, Guild Struggle — so a Cartel cannot arrive without taking its
 * supply with it.
 */
export function gainGuildCard(s: GameState, player: number, cardId: number): void {
  s.playerStates[player].guildCards.push(cardId);
  for (const passive of courtCard(cardId).power?.passives ?? []) {
    if (passive.t === 'cartel') collectCartelSupply(s, passive.resource);
  }
}

/** Take a Guild card off a player, without deciding where it goes next. */
export function removeGuildCard(s: GameState, player: number, cardId: number): void {
  const held = s.playerStates[player].guildCards;
  const i = held.indexOf(cardId);
  if (i >= 0) held.splice(i, 1);
}

/**
 * Discard a Guild card out of play, releasing a Cartel's held supply.
 *
 * Engine ruling: the cards do not say what happens to a Cartel's stockpile when
 * the card leaves play. Returning it to the general supply is the only reading
 * that keeps the tokens in the game.
 */
export function discardGuildCard(s: GameState, player: number, cardId: number): void {
  removeGuildCard(s, player, cardId);
  for (const passive of courtCard(cardId).power?.passives ?? []) {
    if (passive.t === 'cartel') releaseCartelSupply(s, passive.resource);
  }
  s.courtDiscard.push(cardId);
}

/** Move a Guild card between players (raids, Silver-Tongues, Guild Struggle). */
export function stealGuildCard(s: GameState, from: number, to: number, cardId: number): void {
  removeGuildCard(s, from, cardId);
  s.playerStates[to].guildCards.push(cardId);
  // The Cartel's supply travels on the card, so nothing moves between pools.
}

// ---------------------------------------------------------------------------
// Outrage
// ---------------------------------------------------------------------------

/**
 * Provoke Outrage of one type against one player (p16): discard every resource
 * and Guild card of that type, flip the Outrage marker, and lose an agent.
 *
 * Used both when a city is destroyed and by the Vox card Outrage Spreads.
 */
export function provokeOutrage(s: GameState, player: number, type: ResourceType): void {
  const p = s.playerStates[player];
  for (let i = 0; i < p.resources.length; i++) {
    if (p.resources[i] === type) {
      p.resources[i] = null;
      returnToSupply(s, type);
    }
  }
  for (const card of [...p.guildCards]) {
    // "If you Provoke Outrage, keep this card" — the Loyal cards (p20).
    if (courtCard(card).suit === type && !survivesOutrage(card)) discardGuildCard(s, player, card);
  }
  if (!p.outrage[type]) {
    p.outrage[type] = true;
    if (p.agentsSupply > 0) p.agentsSupply--;
  }
}

// ---------------------------------------------------------------------------
// Prelude abilities
// ---------------------------------------------------------------------------

/** Systems a "place N ships" Prelude may target: ones the player controls. */
export function controlledSystems(s: GameState, player: number): number[] {
  const out: number[] = [];
  for (let i = 0; i < s.systems.length; i++) {
    if (s.systems[i].outOfPlay) continue;
    if (controlOf(s, i) === player) out.push(i);
  }
  return out;
}

/** Rival resource slots a steal ability can target. */
function stealTargets(
  s: GameState,
  player: number,
  want: ResourceType | 'any',
): { player: number; slot: number }[] {
  const out: { player: number; slot: number }[] = [];
  for (let p = 0; p < s.players; p++) {
    if (p === player) continue;
    const victim = s.playerStates[p];
    if (theftImmune(victim)) continue;
    for (let slot = 0; slot < openResourceSlots(victim); slot++) {
      const r = victim.resources[slot];
      if (!r) continue;
      if (want !== 'any' && r !== want) continue;
      out.push({ player: p, slot });
    }
  }
  return out;
}

/**
 * Rival Guild cards a steal ability can target.
 *
 * Sworn Guardians shields the rest of its owner's cards but not itself — "in
 * battle they can steal this first and then spend keys" — and the same reading
 * applies to Silver-Tongues and Guild Struggle, which take exactly one card.
 */
function stealCardTargets(s: GameState, player: number): { player: number; card: number }[] {
  const out: { player: number; card: number }[] = [];
  for (let p = 0; p < s.players; p++) {
    if (p === player) continue;
    const victim = s.playerStates[p];
    const shield = theftImmune(victim) ? theftImmunityCard(victim) : null;
    if (shield !== null) {
      out.push({ player: p, card: shield });
      continue;
    }
    for (const card of victim.guildCards) out.push({ player: p, card });
  }
  return out;
}

/** Enumerate the legal `cardPrelude` actions for the player on turn. */
export function preludeCardActions(s: GameState, v: VariantDef, player: number): Action[] {
  const turn = s.turn!;
  const p = s.playerStates[player];
  const acts: Action[] = [];

  for (const card of poweredCards(p)) {
    const ability = card.power!.prelude;
    if (!ability) continue;
    // "You cannot use Prelude actions on cards that you secured from the Court
    // in the same Prelude." (p20)
    if (turn.securedThisPrelude.includes(card.id)) continue;
    if (turn.cardPreludesUsed.includes(card.id)) continue;

    switch (ability.t) {
      case 'placeShips': {
        if (p.shipsSupply <= 0) break;
        for (const system of controlledSystems(s, player)) {
          acts.push({ t: 'cardPrelude', card: card.id, system });
        }
        break;
      }
      case 'shipInEveryGate': {
        if (p.shipsSupply <= 0) break;
        acts.push({ t: 'cardPrelude', card: card.id });
        break;
      }
      case 'fillSlots': {
        if (emptySlots(p) > 0) acts.push({ t: 'cardPrelude', card: card.id });
        break;
      }
      case 'gainResources': {
        if (emptySlots(p) > 0) acts.push({ t: 'cardPrelude', card: card.id });
        break;
      }
      case 'seizeInitiative': {
        if (!s.initiativeSeized && player !== s.initiative) {
          acts.push({ t: 'cardPrelude', card: card.id });
        }
        break;
      }
      case 'stealResource': {
        for (const target of stealTargets(s, player, ability.resource)) {
          acts.push({ t: 'cardPrelude', card: card.id, target: target.player, slot: target.slot });
        }
        break;
      }
      case 'stealAny': {
        // Silver-Tongues: "steal a Guild card or resource".
        for (const target of stealTargets(s, player, 'any')) {
          acts.push({ t: 'cardPrelude', card: card.id, target: target.player, slot: target.slot });
        }
        for (const target of stealCardTargets(s, player)) {
          acts.push({ t: 'cardPrelude', card: card.id, target: target.player, takeCard: target.card });
        }
        break;
      }
      case 'convertResource': {
        // Relic Fence: discard 1 resource to gain 1 Relic, keeping the card.
        if (s.supply[ability.gain] <= 0) break;
        for (let slot = 0; slot < openResourceSlots(p); slot++) {
          if (p.resources[slot]) acts.push({ t: 'cardPrelude', card: card.id, slot });
        }
        break;
      }
      case 'attachUnion': {
        // "place this card next to a face-up played <suit> card"
        s.round.played.forEach((play, i) => {
          if (play.faceDown) return;
          if (actionCard(play.card).suit === ability.suit) {
            acts.push({ t: 'cardPrelude', card: card.id, played: i });
          }
        });
        break;
      }
      case 'recycleHand': {
        // Farseers: discard this plus any subset of the hand, redraw as many.
        for (const cards of subsets(p.hand)) {
          acts.push({ t: 'cardPrelude', card: card.id, cards });
        }
        break;
      }
    }
  }
  return acts;
}

/**
 * Every subset of a hand, smallest first.
 *
 * Hands are at most 6 cards (p5 step P), so this is at most 64 options — the
 * same order as the dice splits a Battle already enumerates. Enumerating them
 * all keeps Farseers' choice the player's rather than an engine ruling.
 */
function subsets(hand: number[]): number[][] {
  const out: number[][] = [];
  const n = Math.min(hand.length, 8);
  for (let mask = 0; mask < 1 << n; mask++) {
    const pick: number[] = [];
    for (let i = 0; i < n; i++) if (mask & (1 << i)) pick.push(hand[i]);
    out.push(pick);
  }
  out.sort((a, b) => a.length - b.length);
  return out;
}

function emptySlots(p: PlayerState): number {
  let n = 0;
  const open = openResourceSlots(p);
  for (let i = 0; i < open; i++) if (p.resources[i] === null) n++;
  return n;
}

/** Apply a `cardPrelude` action. */
export function applyCardPrelude(s: GameState, v: VariantDef, a: Action): void {
  if (a.t !== 'cardPrelude') throw new Error('not a cardPrelude action');
  const turn = s.turn!;
  const player = turn.player;
  const p = s.playerStates[player];
  const card = courtCard(a.card);
  const ability = card.power?.prelude;
  if (!ability) throw new Error(`${card.name} has no Prelude ability`);

  let discard = true;

  switch (ability.t) {
    case 'placeShips': {
      const n = Math.min(ability.count, p.shipsSupply);
      s.systems[a.system!].fresh[player] += n;
      p.shipsSupply -= n;
      break;
    }
    case 'shipInEveryGate': {
      for (const def of v.systems) {
        if (def.kind !== 'gate' || s.systems[def.id].outOfPlay) continue;
        if (p.shipsSupply <= 0) break;
        s.systems[def.id].fresh[player]++;
        p.shipsSupply--;
      }
      break;
    }
    case 'fillSlots': {
      // "gain X up to your number of empty resource slots. If the supply
      // empties, steal the X instead."
      let want = emptySlots(p);
      while (want > 0) {
        if (s.supply[ability.resource] > 0) {
          if (!takeFromSupply(s, player, ability.resource)) break;
        } else {
          const targets = stealTargets(s, player, ability.resource);
          if (targets.length === 0) break;
          stealInto(s, player, targets[0].player, targets[0].slot);
        }
        want--;
      }
      break;
    }
    case 'gainResources': {
      for (const r of ability.resources) takeFromSupply(s, player, r);
      break;
    }
    case 'seizeInitiative': {
      s.round.seizedBy = player;
      s.initiativeSeized = true;
      s.stats.seizes++;
      break;
    }
    case 'stealResource': {
      stealInto(s, player, a.target!, a.slot!);
      break;
    }
    case 'stealAny': {
      if (a.takeCard !== undefined) stealGuildCard(s, a.target!, player, a.takeCard);
      else stealInto(s, player, a.target!, a.slot!);
      break;
    }
    case 'convertResource': {
      const given = p.resources[a.slot!]!;
      p.resources[a.slot!] = null;
      returnToSupply(s, given);
      takeFromSupply(s, player, ability.gain);
      // Relic Fence stays in play; it is once per turn instead.
      discard = false;
      turn.cardPreludesUsed.push(card.id);
      break;
    }
    case 'attachUnion': {
      // The card leaves the play area and sits on the played card until the
      // round ends. It scores nothing while attached, which costs the holder
      // nothing: ambitions only score at a chapter break, by which time every
      // attachment has already resolved.
      removeGuildCard(s, player, card.id);
      s.unions.push({ card: card.id, player, target: s.round.played[a.played!].card });
      discard = false;
      break;
    }
    case 'recycleHand': {
      // "discard this and any number of cards from your hand. Draw the same
      // number of cards (including Farseers) from the bottom of the action
      // discard pile." The count includes Farseers itself, so n discarded hand
      // cards redraw n + 1.
      const give = a.cards ?? [];
      for (const c of give) {
        const i = p.hand.indexOf(c);
        if (i >= 0) p.hand.splice(i, 1);
      }
      // Discarded cards go on top, so they cannot be immediately redrawn.
      s.actionDiscard.push(...give);
      const want = give.length + 1;
      for (let i = 0; i < want; i++) {
        const next = s.actionDiscard.shift();
        if (next === undefined) break;
        p.hand.push(next);
      }
      break;
    }
  }

  if (discard) discardGuildCard(s, player, card.id);
}

/** Move one resource from a Rival's slot into the player's slots. */
function stealInto(s: GameState, player: number, victim: number, slot: number): void {
  const from = s.playerStates[victim];
  const type = from.resources[slot];
  if (!type) return;
  from.resources[slot] = null;
  const to = s.playerStates[player];
  const open = openResourceSlots(to);
  for (let i = 0; i < open; i++) {
    if (to.resources[i] === null) {
      to.resources[i] = type;
      return;
    }
  }
  returnToSupply(s, type); // no room: the token goes back to the supply (p17)
}

// ---------------------------------------------------------------------------
// New actions
// ---------------------------------------------------------------------------

/** New actions a player can afford right now, given the kinds they can pay for. */
export function cardActions(s: GameState, v: VariantDef, player: number, kinds: Set<string>): Action[] {
  const acts: Action[] = [];
  const p = s.playerStates[player];
  for (const card of poweredCards(p)) {
    for (const na of card.power!.newActions ?? []) {
      if (!kinds.has(na.replaces)) continue;
      const base = { t: 'cardAction' as const, card: card.id, name: na.name };

      switch (na.effect.t) {
        case 'gainResource': {
          // Only offer it when the resource can actually be taken.
          if (s.supply[na.effect.resource] <= 0) break;
          if (emptySlots(p) === 0) break;
          acts.push(base);
          break;
        }
        case 'pressgang': {
          // "Return any number of your Captives to gain any 1 resource for each."
          const most = Math.min(p.captives.length, emptySlots(p));
          const available = RESOURCE_TYPES.filter((r) => s.supply[r] > 0);
          for (let k = 1; k <= most; k++) {
            for (const gain of multisets(available, k)) acts.push({ ...base, gain });
          }
          break;
        }
        case 'execute': {
          // "Move any number of your Captives to your Trophies."
          for (let k = 1; k <= p.captives.length; k++) acts.push({ ...base, count: k });
          break;
        }
        case 'abduct': {
          // "Capture all Rival agents from a card in the Court that has fewer
          // Rival agents than your total Weapon icons."
          const icons = weaponIcons(p);
          s.court.forEach((slot, i) => {
            const rivals = slot.agents.reduce((n, c, pl) => (pl === player ? n : n + c), 0);
            if (rivals > 0 && rivals < icons) acts.push({ ...base, slot: i });
          });
          break;
        }
        case 'trade': {
          acts.push(...tradeActions(s, v, player, base));
          break;
        }
      }
    }
  }
  return acts;
}

/**
 * Elder Broker's Trade: "Choose a Rival city you control. Swap 1 resource with
 * that Rival — take a resource of that city type from them, and give them a
 * resource they don't have."
 *
 * Engine ruling: Sworn Guardians does **not** block this. Its text is "Rivals
 * cannot steal your resources", and the rulebook uses *steal* as a keyword —
 * raids steal, Silver-Tongues steals. Trade says "swap", and hands something
 * back, so it is not a theft.
 */
function tradeActions(
  s: GameState,
  v: VariantDef,
  player: number,
  base: { t: 'cardAction'; card: number; name: string },
): Action[] {
  const acts: Action[] = [];
  const me = s.playerStates[player];
  for (const def of v.systems) {
    const st = s.systems[def.id];
    if (st.outOfPlay || def.planetType === null) continue;
    if (controlOf(s, def.id) !== player) continue;
    st.buildings.forEach((b, building) => {
      if (b.kind !== 'city' || b.player === player) return;
      const them = s.playerStates[b.player];
      // Their resource of the city's type.
      const takeSlot = them.resources.findIndex((r, i) => r === def.planetType && i < openResourceSlots(them));
      if (takeSlot < 0) return;
      // One of mine of a type they do not hold.
      const theirTypes = new Set(them.resources.filter((r) => r !== null));
      for (let giveSlot = 0; giveSlot < openResourceSlots(me); giveSlot++) {
        const mine = me.resources[giveSlot];
        if (!mine || theirTypes.has(mine)) continue;
        acts.push({ ...base, system: def.id, building, slot: takeSlot, giveSlot });
      }
    });
  }
  return acts;
}

/**
 * Every multiset of `k` items from `types`, as sorted arrays.
 *
 * Pressgang gains one resource per returned Captive, freely chosen, so the
 * option set really is the multisets. `k` is bounded by empty resource slots
 * (at most 6, usually 0-3), which keeps this small in practice.
 */
function multisets(types: ResourceType[], k: number): ResourceType[][] {
  if (k === 0) return [[]];
  const out: ResourceType[][] = [];
  for (let i = 0; i < types.length; i++) {
    for (const rest of multisets(types.slice(i), k - 1)) out.push([types[i], ...rest]);
  }
  return out;
}

/** The standard action a card action is paid for with. */
export function cardActionCost(cardId: number, name: string): 'build' | 'influence' | 'battle' | 'tax' {
  const na = courtCard(cardId).power?.newActions?.find((x) => x.name === name);
  if (!na) throw new Error(`unknown card action ${name}`);
  return na.replaces;
}

export function applyCardAction(s: GameState, player: number, a: Action): void {
  if (a.t !== 'cardAction') throw new Error('not a cardAction');
  const na = courtCard(a.card).power?.newActions?.find((x) => x.name === a.name);
  if (!na) throw new Error(`unknown card action ${a.name}`);
  const p = s.playerStates[player];

  switch (na.effect.t) {
    case 'gainResource': {
      takeFromSupply(s, player, na.effect.resource);
      return;
    }
    case 'pressgang': {
      // One Captive returns to its owner's supply per resource gained.
      for (const r of a.gain ?? []) {
        const owner = p.captives.pop();
        if (owner === undefined) break;
        s.playerStates[owner].agentsSupply++;
        takeFromSupply(s, player, r);
      }
      return;
    }
    case 'execute': {
      // Engine ruling: the Captives moved are taken in the order they were
      // captured. Which Rival they belong to only matters when Warlord scores
      // and Trophies go home, and the card gives the player no say in it.
      const n = Math.min(a.count ?? 0, p.captives.length);
      for (let i = 0; i < n; i++) {
        const owner = p.captives.pop()!;
        p.trophies.push({ owner, kind: 'agent' });
      }
      return;
    }
    case 'abduct': {
      const slot = s.court[a.slot!];
      for (let pl = 0; pl < s.players; pl++) {
        if (pl === player) continue;
        for (let i = 0; i < slot.agents[pl]; i++) p.captives.push(pl);
        slot.agents[pl] = 0;
      }
      return;
    }
    case 'trade': {
      const them = s.playerStates[s.systems[a.system!].buildings[a.building!].player];
      const taken = them.resources[a.slot!];
      const given = p.resources[a.giveSlot!];
      if (!taken || !given) return;
      them.resources[a.slot!] = given;
      p.resources[a.giveSlot!] = taken;
      return;
    }
  }
}

// ---------------------------------------------------------------------------
// Farseers: look at a Rival hand and maybe swap
// ---------------------------------------------------------------------------

/**
 * "When you declare an ambition, look at a Rival's hand. You may swap 1 card
 * with them."
 *
 * This is two decisions, not one, and deliberately so: choosing whom to look at
 * has to be committed *before* the hand is revealed, or the ability would leak
 * every Rival's hand instead of the one it names. `observe()` reveals only the
 * chosen target's hand, and only while the swap is pending.
 */
export function peekTargetActions(s: GameState, player: number): Action[] {
  const acts: Action[] = [];
  for (let p = 0; p < s.players; p++) {
    if (p === player) continue;
    if (s.playerStates[p].hand.length > 0) acts.push({ t: 'peekTarget', target: p });
  }
  acts.push({ t: 'peekTarget', target: null });
  return acts;
}

export function peekSwapActions(s: GameState): Action[] {
  const peek = s.peek!;
  const mine = s.playerStates[peek.player].hand;
  const theirs = s.playerStates[peek.target!].hand;
  const acts: Action[] = [];
  for (const give of mine) for (const take of theirs) acts.push({ t: 'peekSwap', give, take });
  acts.push({ t: 'peekSwapSkip' });
  return acts;
}

export function applyPeekSwap(s: GameState, a: Action): void {
  if (a.t !== 'peekSwap') return;
  const peek = s.peek!;
  const mine = s.playerStates[peek.player].hand;
  const theirs = s.playerStates[peek.target!].hand;
  const i = mine.indexOf(a.give);
  const j = theirs.indexOf(a.take);
  if (i < 0 || j < 0) return;
  mine[i] = a.take;
  theirs[j] = a.give;
}

// ---------------------------------------------------------------------------
// Vox `When Secured`
// ---------------------------------------------------------------------------

/**
 * Does this Vox card need a decision, or can it resolve on the spot?
 *
 * Call to Action just draws a card, so it resolves inline and never blocks the
 * turn; the other five each ask the securing player something.
 */
export function voxNeedsDecision(cardId: number): boolean {
  return courtCard(cardId).vox?.t !== 'drawFromDiscardBottom';
}

/** Resolve the Vox effects that need no decision. */
export function applyVoxImmediate(s: GameState, player: number, cardId: number): void {
  const effect = courtCard(cardId).vox;
  if (effect?.t !== 'drawFromDiscardBottom') throw new Error('vox needs a decision');
  const next = s.actionDiscard.shift();
  if (next !== undefined) s.playerStates[player].hand.push(next);
  s.courtDiscard.push(cardId);
}

/** Enumerate the choices for the pending Vox card. */
export function voxActions(s: GameState): Action[] {
  const pending = s.pendingVox!;
  const player = pending.player;
  const p = s.playerStates[player];
  const effect = courtCard(pending.card).vox!;
  const acts: Action[] = [];

  switch (effect.t) {
    case 'shipInEachSystemOfCluster': {
      // "Choose a cluster on the map. You place 1 ship in each system of that
      // cluster." Not optional, so only offer a skip when nothing is possible.
      if (p.shipsSupply > 0) {
        for (let c = 0; c < CLUSTER_COUNT; c++) {
          if (s.systems.some((st, i) => !st.outOfPlay && clusterOf(i) === c)) {
            acts.push({ t: 'vox', cluster: c });
          }
        }
      }
      break;
    }
    case 'declareAnyAmbition': {
      if (s.availableMarkers.length > 0) {
        for (const ambition of AMBITIONS) acts.push({ t: 'vox', ambition });
      }
      break;
    }
    case 'outrageAll': {
      for (const resource of RESOURCE_TYPES) acts.push({ t: 'vox', resource });
      break;
    }
    case 'returnCityMaySeize': {
      for (let system = 0; system < s.systems.length; system++) {
        if (s.systems[system].outOfPlay) continue;
        if (controlOf(s, system) !== player) continue;
        s.systems[system].buildings.forEach((b, building) => {
          if (b.kind !== 'city') return;
          acts.push({ t: 'vox', system, building, seize: false });
          if (!s.initiativeSeized && player !== s.initiative) {
            acts.push({ t: 'vox', system, building, seize: true });
          }
        });
      }
      break;
    }
    case 'stealGuildCardAndRecycle': {
      for (const target of stealCardTargets(s, player)) {
        acts.push({ t: 'vox', target: target.player, card: target.card });
      }
      break;
    }
    case 'drawFromDiscardBottom':
      break;
  }

  // Mass Uprising is the one mandatory choice — "Choose a cluster ... You place
  // 1 ship in each system" — so it only allows declining when nothing is legal.
  // The other four all read "You may".
  if (effect.t !== 'shipInEachSystemOfCluster' || acts.length === 0) {
    acts.push({ t: 'voxSkip' });
  }
  return acts;
}

/**
 * Apply a Vox choice and put the card away. Returns the ambition Populist
 * Demands declared, if any — declaring lives in the state machine, because it
 * pulls a marker and can trigger Farseers.
 *
 * Engine ruling: two of these say "shuffle this card into the Court deck" and
 * one says "shuffle all Guild cards from the Court discard pile into the Court
 * deck". Securing is a decision node with no RNG in scope by design (see
 * `secureCard`), so the cards go to the *bottom* of the deck in a defined
 * order instead. The Court deck's order is hidden information either way, and
 * `determinize()` reshuffles it for search.
 */
export function applyVox(s: GameState, a: Action): { declare?: AmbitionId } {
  const pending = s.pendingVox!;
  const player = pending.player;
  const p = s.playerStates[player];
  const effect = courtCard(pending.card).vox!;
  const declined = a.t === 'voxSkip';
  const out: { declare?: AmbitionId } = {};

  switch (effect.t) {
    case 'shipInEachSystemOfCluster': {
      if (!declined && a.t === 'vox') {
        for (let i = 0; i < s.systems.length; i++) {
          if (s.systems[i].outOfPlay || clusterOf(i) !== a.cluster) continue;
          if (p.shipsSupply <= 0) break;
          s.systems[i].fresh[player]++;
          p.shipsSupply--;
        }
      }
      s.courtDiscard.push(pending.card);
      break;
    }
    case 'declareAnyAmbition': {
      if (!declined && a.t === 'vox' && a.ambition) out.declare = a.ambition;
      s.courtDiscard.push(pending.card);
      break;
    }
    case 'outrageAll': {
      // "Each player (even you) must Provoke Outrage of that type."
      if (!declined && a.t === 'vox') {
        for (let pl = 0; pl < s.players; pl++) provokeOutrage(s, pl, a.resource!);
      }
      s.courtDiscard.push(pending.card);
      break;
    }
    case 'returnCityMaySeize': {
      if (!declined && a.t === 'vox') {
        const st = s.systems[a.system!];
        const b = st.buildings[a.building!];
        if (b && b.kind === 'city') {
          st.buildings.splice(a.building!, 1);
          const owner = s.playerStates[b.player];
          owner.citiesUsed = Math.max(0, owner.citiesUsed - 1);
          compactResources(owner);
          if (a.seize && !s.initiativeSeized && player !== s.initiative) {
            s.round.seizedBy = player;
            s.initiativeSeized = true;
            s.stats.seizes++;
          }
        }
      }
      // "Shuffle this card into the Court deck" — bottom, per the ruling above.
      s.courtDeck.unshift(pending.card);
      break;
    }
    case 'stealGuildCardAndRecycle': {
      if (!declined && a.t === 'vox') stealGuildCard(s, a.target!, player, a.card!);
      // "Shuffle all Guild cards from the Court discard pile into the Court deck."
      const guild = s.courtDiscard.filter((id) => courtCard(id).kind === 'guild');
      s.courtDiscard = s.courtDiscard.filter((id) => courtCard(id).kind !== 'guild');
      s.courtDeck.unshift(...guild);
      s.courtDiscard.push(pending.card);
      break;
    }
    case 'drawFromDiscardBottom': {
      applyVoxImmediate(s, player, pending.card);
      break;
    }
  }

  s.pendingVox = null;
  return out;
}

/** Court cards that exist, for tests and the UI. */
export { COURT_DECK };
export { hasPiece };
