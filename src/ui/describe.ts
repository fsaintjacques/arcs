/** Human-readable labels for actions and state, shared by the UI. */
import {
  actionCard,
  courtCard,
  type Action,
  type AmbitionId,
  type GameState,
  type ResourceType,
  type VariantDef,
} from '../engine';

export const SUIT_SHORT: Record<string, string> = {
  administration: 'Admin',
  aggression: 'Aggro',
  construction: 'Const',
  mobilization: 'Mobil',
};

export const RESOURCE_ICON: Record<ResourceType, string> = {
  material: '▧',
  fuel: '◍',
  weapon: '⚔',
  relic: '✦',
  psionic: '◉',
};

export const AMBITION_LABEL: Record<AmbitionId, string> = {
  tycoon: 'Tycoon',
  tyrant: 'Tyrant',
  warlord: 'Warlord',
  keeper: 'Keeper',
  empath: 'Empath',
};

export const AMBITION_COUNTS: Record<AmbitionId, string> = {
  tycoon: 'Fuel + Material icons',
  tyrant: 'Captives',
  warlord: 'Trophies',
  keeper: 'Relic icons',
  empath: 'Psionic icons',
};

export const PLAYER_COLORS = ['#e05c4a', '#4a9ee0', '#7bc06a', '#e0b84a'];
export const PLAYER_NAMES = ['Red', 'Blue', 'Green', 'Yellow'];

export function cardLabel(id: number): string {
  const c = actionCard(id);
  return `${SUIT_SHORT[c.suit]} ${c.number}`;
}

export function cardPips(id: number): number {
  return actionCard(id).pips;
}

export function systemLabel(v: VariantDef, id: number): string {
  const s = v.systems[id];
  return s.kind === 'gate' ? `Gate ${s.label}` : `${s.label} ${s.planetType}`;
}

/** A short, self-explanatory label for one legal action. */
export function describeAction(a: Action, s: GameState, v: VariantDef): string {
  switch (a.t) {
    case 'lead':
      return `Lead ${cardLabel(a.card)} — ${cardPips(a.card)} action${cardPips(a.card) === 1 ? '' : 's'}`;
    case 'follow': {
      const pips = a.mode === 'surpass' ? cardPips(a.card) : 1;
      const suffix =
        a.mode === 'copy'
          ? 'face down, 1 action of the lead card'
          : `${pips} action${pips === 1 ? '' : 's'}`;
      return `${a.mode[0].toUpperCase()}${a.mode.slice(1)} with ${cardLabel(a.card)} — ${suffix}`;
    }
    case 'passInitiative':
      return 'Pass the initiative (ends the round)';
    case 'mulligan':
      return a.take ? 'Mulligan: draw 6 new cards' : 'Keep this hand';
    case 'declareAmbition':
      return `Declare ${AMBITION_LABEL[a.ambition]} — ${AMBITION_COUNTS[a.ambition]}`;
    case 'seize':
      return `Seize the initiative, burning ${cardLabel(a.card)}`;
    case 'spendResource': {
      const r = s.playerStates[s.turn!.player].resources[a.slot];
      const effect: Record<string, string> = {
        material: 'a Build or Repair',
        fuel: 'a Move',
        weapon: 'Battle actions from your pips this turn',
        relic: 'a Secure',
        psionic: 'an action from the lead card',
      };
      return `Spend ${r} — ${effect[r ?? '']}`;
    }
    case 'spendResourceAs': {
      const r = s.playerStates[s.turn!.player].resources[a.slot];
      return `Spend ${r} as ${a.as} (Loyal card)`;
    }
    case 'cardPrelude': {
      const card = courtCard(a.card);
      if (a.played !== undefined) {
        return `${card.name}: attach to ${cardLabel(s.round.played[a.played].card)} to draw it at round end`;
      }
      if (a.cards !== undefined) {
        const n = a.cards.length;
        return `${card.name}: discard ${n} card${n === 1 ? '' : 's'}, draw ${n + 1} from the discard bottom`;
      }
      const what = a.takeCard !== undefined ? ` ${courtCard(a.takeCard).name}` : '';
      const where = a.system !== undefined ? ` at ${systemLabel(v, a.system)}` : '';
      const from = a.target !== undefined ? ` from ${PLAYER_NAMES[a.target]}` : '';
      return `${card.name} Prelude${what}${where}${from}`;
    }
    case 'cardAction': {
      const name = courtCard(a.card).name;
      if (a.gain) return `${a.name} — return ${a.gain.length} Captive(s) for ${a.gain.join(', ')}`;
      if (a.name === 'Execute') return `${a.name} — move ${a.count} Captive(s) to Trophies`;
      if (a.name === 'Abduct') return `${a.name} — take Rival agents off ${courtCard(s.court[a.slot!].card).name}`;
      if (a.name === 'Trade') {
        const them = s.systems[a.system!].buildings[a.building!].player;
        return `${a.name} — swap with ${PLAYER_NAMES[them]} at ${systemLabel(v, a.system!)}`;
      }
      return `${a.name} — ${name}`;
    }
    case 'rerollSkirmish':
      return a.count === 0
        ? 'Keep the roll'
        : `Reroll ${a.count} blank skirmish di${a.count === 1 ? 'e' : 'ce'}`;
    case 'peekTarget':
      return a.target === null
        ? 'Do not look at anyone’s hand'
        : `Look at ${PLAYER_NAMES[a.target]}’s hand`;
    case 'peekSwap':
      return `Give ${cardLabel(a.give)}, take ${cardLabel(a.take)}`;
    case 'peekSwapSkip':
      return 'Swap nothing';
    case 'vox': {
      const name = courtCard(s.pendingVox!.card).name;
      if (a.cluster !== undefined) return `${name}: 1 ship in every system of cluster ${a.cluster + 1}`;
      if (a.ambition !== undefined) return `${name}: declare ${AMBITION_LABEL[a.ambition]}`;
      if (a.resource !== undefined) return `${name}: everyone Provokes Outrage of ${a.resource}`;
      if (a.system !== undefined) {
        const owner = s.systems[a.system].buildings[a.building!]?.player ?? 0;
        return `${name}: return ${PLAYER_NAMES[owner]}’s city at ${systemLabel(v, a.system)}${a.seize ? ' and seize' : ''}`;
      }
      if (a.card !== undefined) return `${name}: steal ${courtCard(a.card).name} from ${PLAYER_NAMES[a.target!]}`;
      return name;
    }
    case 'voxSkip':
      return `Decline ${courtCard(s.pendingVox!.card).name}`;
    case 'beginActions':
      return 'End Prelude, start spending pips';
    case 'tax': {
      const b = s.systems[a.system].buildings[a.building];
      const owner = b.player === s.turn?.player ? 'your' : `${PLAYER_NAMES[b.player]}'s`;
      return `Tax ${owner} city at ${systemLabel(v, a.system)}`;
    }
    case 'buildShip':
      return `Build a ship at your starport in ${systemLabel(v, a.system)}`;
    case 'buildBuilding':
      return `Build a ${a.kind} at ${systemLabel(v, a.system)}`;
    case 'move':
      return `Move ${a.ships} ship${a.ships === 1 ? '' : 's'}: ${systemLabel(v, a.from)} → ${systemLabel(v, a.to)}`;
    case 'catapult':
      return `Catapult ${a.ships} onward to ${systemLabel(v, a.to)}`;
    case 'catapultStop':
      return 'Stop here';
    case 'repair':
      return a.building === null
        ? `Repair a ship at ${systemLabel(v, a.system)}`
        : `Repair your ${s.systems[a.system].buildings[a.building].kind} at ${systemLabel(v, a.system)}`;
    case 'influence':
      return `Influence ${courtCard(s.court[a.slot].card).name}`;
    case 'secure':
      return `Secure ${courtCard(s.court[a.slot].card).name}`;
    case 'battle': {
      const dice = [
        a.assault ? `${a.assault} assault` : '',
        a.skirmish ? `${a.skirmish} skirmish` : '',
        a.raid ? `${a.raid} raid` : '',
      ]
        .filter(Boolean)
        .join(', ');
      return `Battle ${PLAYER_NAMES[a.defender]} at ${systemLabel(v, a.system)} — ${dice}`;
    }
    case 'assignSelf':
      return `Take a hit on one of your ${a.fresh ? 'fresh' : 'damaged'} ships${a.fresh ? '' : ' (destroyed)'}`;
    case 'assignHit':
      return a.target === 'ship'
        ? `Hit a ${a.fresh ? 'fresh' : 'damaged'} defending ship${a.fresh ? '' : ' (destroyed)'}`
        : `Hit their ${s.systems[s.battle!.system].buildings[a.building]?.kind ?? 'building'}`;
    case 'raidResource':
      return `Steal their ${s.playerStates[s.battle!.defender].resources[a.slot]}`;
    case 'raidCard':
      return `Steal ${courtCard(a.card).name}`;
    case 'raidDone':
      return 'Stop raiding';
    case 'reinforce':
      return `Place 3 ships in ${systemLabel(v, a.system)}`;
    case 'endTurn':
      return 'End turn';
  }
}

/** Group heading an action belongs under, so long lists stay navigable. */
export function actionGroup(a: Action): string {
  switch (a.t) {
    case 'lead':
    case 'follow':
    case 'passInitiative':
      return 'Play a card';
    case 'declareAmbition':
      return 'Declare an ambition';
    case 'seize':
      return 'Seize the initiative';
    case 'spendResource':
    case 'spendResourceAs':
    case 'cardPrelude':
      return 'Prelude: spend resources';
    case 'cardAction':
      return 'Guild card action';
    case 'tax':
      return 'Tax';
    case 'buildShip':
    case 'buildBuilding':
      return 'Build';
    case 'move':
    case 'catapult':
      return 'Move';
    case 'repair':
      return 'Repair';
    case 'influence':
      return 'Influence';
    case 'secure':
      return 'Secure';
    case 'battle':
      return 'Battle';
    case 'assignSelf':
    case 'assignHit':
      return 'Assign hits';
    case 'raidResource':
    case 'raidCard':
      return 'Raid';
    case 'rerollSkirmish':
      return 'Skirmishers: reroll';
    case 'peekTarget':
    case 'peekSwap':
    case 'peekSwapSkip':
      return 'Farseers';
    case 'vox':
    case 'voxSkip':
      return 'Vox: when secured';
    default:
      return 'Finish';
  }
}

/** The system an action points at, for board highlighting. */
export function actionSystem(a: Action): number | null {
  switch (a.t) {
    case 'tax':
    case 'buildShip':
    case 'buildBuilding':
    case 'repair':
    case 'battle':
    case 'reinforce':
      return a.system;
    case 'move':
      return a.to;
    case 'catapult':
      return a.to;
    default:
      return null;
  }
}

export function phaseLabel(s: GameState): string {
  if (s.pendingVox) return `${courtCard(s.pendingVox.card).name} — when secured`;
  switch (s.phase) {
    case 'deal':
      return 'Dealing a new chapter';
    case 'mulligan':
      return 'Mulligan';
    case 'play':
      return s.round.lead ? 'Follow the lead card' : 'Lead a card';
    case 'prelude':
      return 'Prelude';
    case 'actions':
      return `Actions — ${s.turn?.pipsLeft ?? 0} pip${s.turn?.pipsLeft === 1 ? '' : 's'} left`;
    case 'catapult':
      return 'Catapult';
    case 'battleRoll':
      return 'Rolling battle dice';
    case 'battleReroll':
      return 'Skirmishers — reroll blanks?';
    case 'battleAssign':
      return 'Assigning hits';
    case 'peekTarget':
    case 'peekSwap':
      return 'Farseers — looking at a hand';
    case 'reinforce':
      return 'Reinforcing';
    case 'over':
      return 'Game over';
    default:
      return s.phase;
  }
}
