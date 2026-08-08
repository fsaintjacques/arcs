/**
 * Guild card ability dispatch (rulebook p20).
 *
 * The rulebook sorts card powers into three kinds, and this module handles
 * each where the state machine reaches for it:
 *
 *   - **`Prelude:` abilities** — enumerated in the prelude phase, most of them
 *     discarding the card to do something once.
 *   - **New actions** written `Name (Standard):` — offered wherever the
 *     standard action they replace is affordable, and paid for the same way.
 *   - **Passive modifiers** — queried by the engine at the point they apply
 *     (extra battle dice, theft immunity, the zero marker, spending a resource
 *     as another type).
 *
 * A card only counts as implemented once it is listed in `IMPLEMENTED_POWERS`
 * and covered by a test; `UNIMPLEMENTED_POWERS` is the complement.
 */
import { courtCard } from './court';
import { controlOf, hasPiece, returnToSupply, takeFromSupply } from './board';
import { isGate } from './map';
import { openResourceSlots } from './playerBoard';
import type { Action, GameState, PlayerState, ResourceType, VariantDef } from './types';

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

/** Secret Order: declaring Keeper or Empath does not place the zero marker. */
export function skipsZeroMarker(p: PlayerState, ambition: string): boolean {
  for (const c of poweredCards(p)) {
    for (const passive of c.power!.passives ?? []) {
      if (passive.t === 'noZeroMarker' && passive.ambitions === 'keeperEmpath') {
        if (ambition === 'keeper' || ambition === 'empath') return true;
      }
    }
  }
  return false;
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

/** Rivals a steal ability can target, given what they hold. */
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
        const empty = emptySlots(p);
        if (empty > 0) acts.push({ t: 'cardPrelude', card: card.id });
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
      case 'stealResource':
      case 'stealAny': {
        const want = ability.t === 'stealResource' ? ability.resource : ('any' as const);
        for (const target of stealTargets(s, player, want)) {
          acts.push({ t: 'cardPrelude', card: card.id, target: target.player, slot: target.slot });
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
    }
  }
  return acts;
}

function emptySlots(p: PlayerState): number {
  let n = 0;
  const open = openResourceSlots(p);
  for (let i = 0; i < open; i++) if (p.resources[i] === null) n++;
  return n;
}

/** Apply a `cardPrelude` action. Returns false if the card should be kept. */
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
    case 'stealResource':
    case 'stealAny': {
      stealInto(s, player, a.target!, a.slot!);
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
  }

  if (discard) {
    p.guildCards.splice(p.guildCards.indexOf(card.id), 1);
    s.courtDiscard.push(card.id);
  }
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
export function cardActions(s: GameState, player: number, kinds: Set<string>): Action[] {
  const acts: Action[] = [];
  for (const card of poweredCards(s.playerStates[player])) {
    for (const na of card.power!.newActions ?? []) {
      if (!kinds.has(na.replaces)) continue;
      if (na.effect.t === 'gainResource') {
        // Only offer it when the resource can actually be taken.
        if (s.supply[na.effect.resource] <= 0) continue;
        if (emptySlots(s.playerStates[player]) === 0) continue;
      } else {
        continue; // effect not dispatched yet
      }
      acts.push({ t: 'cardAction', card: card.id, name: na.name });
    }
  }
  return acts;
}

/** The standard action a card action is paid for with. */
export function cardActionCost(cardId: number, name: string): 'build' | 'influence' | 'battle' | 'tax' {
  const na = courtCard(cardId).power?.newActions?.find((x) => x.name === name);
  if (!na) throw new Error(`unknown card action ${name}`);
  return na.replaces;
}

export function applyCardAction(s: GameState, player: number, cardId: number, name: string): void {
  const na = courtCard(cardId).power?.newActions?.find((x) => x.name === name);
  if (!na) throw new Error(`unknown card action ${name}`);
  if (na.effect.t === 'gainResource') {
    takeFromSupply(s, player, na.effect.resource);
    return;
  }
  throw new Error(`card action ${name} is not implemented`);
}

export { hasPiece };
