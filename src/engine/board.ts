/**
 * Board queries shared by the state machine, the bots and the UI.
 */
import type { BuildingKind, GameState, ResourceType, SystemDef, VariantDef } from './types';
import { gainResource } from './playerBoard';

/**
 * "You control a system and its contents if you have more fresh ships there
 * than each Rival. On a tie, no one controls the system." (p7)
 */
export function controlOf(s: GameState, system: number): number | null {
  const fresh = s.systems[system].fresh;
  let best = -1;
  let winner: number | null = null;
  let tied = false;
  for (let p = 0; p < fresh.length; p++) {
    if (fresh[p] > best) {
      best = fresh[p];
      winner = p;
      tied = false;
    } else if (fresh[p] === best) {
      tied = true;
    }
  }
  if (best <= 0 || tied) return null;
  return winner;
}

export function shipsOf(s: GameState, system: number, player: number): number {
  return s.systems[system].fresh[player] + s.systems[system].damaged[player];
}

export function totalShips(s: GameState, system: number): number {
  const st = s.systems[system];
  return st.fresh.reduce((a, b) => a + b, 0) + st.damaged.reduce((a, b) => a + b, 0);
}

/** Does the player have any piece (ship or building) in the system? */
export function hasPiece(s: GameState, system: number, player: number): boolean {
  if (shipsOf(s, system, player) > 0) return true;
  return s.systems[system].buildings.some((b) => b.player === player);
}

export function hasBuilding(
  s: GameState,
  system: number,
  player: number,
  kind?: BuildingKind,
): boolean {
  return s.systems[system].buildings.some(
    (b) => b.player === player && (kind === undefined || b.kind === kind),
  );
}

/** Free building slots left on a planet (gates have none). */
export function freeBuildingSlots(s: GameState, def: SystemDef): number {
  return def.buildingSlots - s.systems[def.id].buildings.length;
}

/** Systems a player still occupies with a ship or starport (p22, elimination). */
export function isWipedOut(s: GameState, player: number): boolean {
  for (let i = 0; i < s.systems.length; i++) {
    if (s.systems[i].outOfPlay) continue;
    if (shipsOf(s, i, player) > 0) return false;
    if (hasBuilding(s, i, player, 'starport')) return false;
  }
  return true;
}

/** Does the defender hold any building anywhere? Gates the raid dice (p14). */
export function hasAnyBuilding(s: GameState, player: number): boolean {
  return s.systems.some((st) => !st.outOfPlay && st.buildings.some((b) => b.player === player));
}

/** All in-play systems adjacent to `system`. */
export function neighbours(v: VariantDef, s: GameState, system: number): number[] {
  return v.systems[system].adjacent.filter((n) => !s.systems[n].outOfPlay);
}

/** Take a resource from the general supply into a player's slots. */
export function takeFromSupply(s: GameState, player: number, type: ResourceType): boolean {
  if (s.supply[type] <= 0) return false;
  if (!gainResource(s.playerStates[player], type)) return false;
  s.supply[type]--;
  return true;
}

/** Return a resource token to the general supply. */
export function returnToSupply(s: GameState, type: ResourceType): void {
  s.supply[type]++;
}

/** Every player other than `player` with a piece in the system. */
export function rivalsIn(s: GameState, system: number, player: number): number[] {
  const out: number[] = [];
  for (let p = 0; p < s.players; p++) {
    if (p === player) continue;
    if (hasPiece(s, system, p)) out.push(p);
  }
  return out;
}

/** Deep copy — hot enough that this beats structuredClone by a wide margin. */
export function cloneState(s: GameState): GameState {
  return {
    ...s,
    systems: s.systems.map((sys) => ({
      fresh: sys.fresh.slice(),
      damaged: sys.damaged.slice(),
      buildings: sys.buildings.map((b) => ({ ...b })),
      outOfPlay: sys.outOfPlay,
    })),
    playerStates: s.playerStates.map((p) => ({
      ...p,
      resources: p.resources.slice(),
      outrage: { ...p.outrage },
      guildCards: p.guildCards.slice(),
      trophies: p.trophies.map((t) => ({ ...t })),
      captives: p.captives.slice(),
      hand: p.hand.slice(),
    })),
    supply: { ...s.supply },
    court: s.court.map((c) => ({ card: c.card, agents: c.agents.slice() })),
    courtDeck: s.courtDeck.slice(),
    courtDiscard: s.courtDiscard.slice(),
    actionDeck: s.actionDeck.slice(),
    actionDiscard: s.actionDiscard.slice(),
    round: {
      ...s.round,
      turnOrder: s.round.turnOrder.slice(),
      lead: s.round.lead ? { ...s.round.lead } : null,
      played: s.round.played.map((p) => ({ ...p })),
    },
    turn: s.turn
      ? {
          ...s.turn,
          pipActions: s.turn.pipActions.slice(),
          freeActions: s.turn.freeActions.map((f) => f.slice()),
          preludeSpent: s.turn.preludeSpent.slice(),
          securedThisPrelude: s.turn.securedThisPrelude.slice(),
          cardPreludesUsed: s.turn.cardPreludesUsed.slice(),
        }
      : null,
    battle: s.battle ? { ...s.battle, dice: { ...s.battle.dice } } : null,
    move: s.move ? { ...s.move } : null,
    declared: {
      tycoon: s.declared.tycoon.slice(),
      tyrant: s.declared.tyrant.slice(),
      warlord: s.declared.warlord.slice(),
      keeper: s.declared.keeper.slice(),
      empath: s.declared.empath.slice(),
    },
    availableMarkers: s.availableMarkers.slice(),
    flipped: s.flipped.slice(),
    phantom: { ...s.phantom },
    stats: { ...s.stats },
  };
}
