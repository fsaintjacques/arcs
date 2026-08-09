/**
 * Canonical action keys.
 *
 * Search agents key tree nodes by action; `JSON.stringify` worked but is
 * slow, and its output depends on the key order the literal happened to be
 * built with. `encodeAction` is a stable, compact encoding with a switch the
 * compiler forces to stay exhaustive: adding an Action variant breaks the
 * build here instead of silently colliding node keys.
 *
 * Two actions encode equal iff they are the same choice. Optional fields
 * are encoded with a presence marker where their absence is meaningful
 * (Farseers' `cards: []` discards nothing but still recycles).
 */
import type { Action } from './types';

const opt = (x: number | string | undefined): string => (x === undefined ? '' : String(x));
const arr = (x: readonly (number | string)[] | undefined): string =>
  x === undefined ? '' : `[${x.join('.')}]`;

export function encodeAction(a: Action): string {
  switch (a.t) {
    case 'lead':
      return `ld:${a.card}`;
    case 'follow':
      return `fo:${a.card}:${a.mode[0]}`;
    case 'passInitiative':
      return 'pi';
    case 'mulligan':
      return `mu:${a.take ? 1 : 0}`;
    case 'declareAmbition':
      return `da:${a.ambition}`;
    case 'seize':
      return `sz:${a.card}`;
    case 'spendResource':
      return `sr:${a.slot}`;
    case 'spendResourceAs':
      return `sa:${a.slot}:${a.as}`;
    case 'cardPrelude':
      return `cp:${a.card}:${opt(a.system)}:${opt(a.slot)}:${opt(a.target)}:${opt(a.takeCard)}:${opt(a.played)}:${arr(a.cards)}`;
    case 'beginActions':
      return 'ba';
    case 'tax':
      return `tx:${a.system}:${a.building}`;
    case 'buildShip':
      return `bs:${a.system}:${a.building}`;
    case 'buildBuilding':
      return `bb:${a.system}:${a.kind[0]}`;
    case 'cardAction':
      return `ca:${a.card}:${a.name}:${arr(a.gain)}:${opt(a.count)}:${opt(a.slot)}:${opt(a.system)}:${opt(a.building)}:${opt(a.giveSlot)}`;
    case 'move':
      return `mv:${a.from}:${a.to}:${a.ships}`;
    case 'catapult':
      return `ct:${a.to}:${a.ships}`;
    case 'catapultStop':
      return 'cs';
    case 'repair':
      return `rp:${a.system}:${a.building === null ? 'n' : a.building}`;
    case 'influence':
      return `in:${a.slot}`;
    case 'secure':
      return `se:${a.slot}`;
    case 'battle':
      return `bt:${a.system}:${a.defender}:${a.assault}/${a.skirmish}/${a.raid}`;
    case 'endTurn':
      return 'et';
    case 'assignSelf':
      return `as:${a.fresh ? 1 : 0}`;
    case 'assignHit':
      return a.target === 'ship' ? `ah:s:${a.fresh ? 1 : 0}` : `ah:b:${a.building}`;
    case 'raidResource':
      return `rr:${a.slot}`;
    case 'raidCard':
      return `rc:${a.card}`;
    case 'raidDone':
      return 'rd';
    case 'rerollSkirmish':
      return `rs:${a.count}`;
    case 'peekTarget':
      return `pt:${a.target === null ? 'n' : a.target}`;
    case 'peekSwap':
      return `ps:${a.give}:${a.take}`;
    case 'peekSwapSkip':
      return 'pk';
    case 'vox':
      return `vx:${opt(a.cluster)}:${opt(a.ambition)}:${opt(a.resource)}:${opt(a.system)}:${opt(a.building)}:${a.seize === undefined ? '' : a.seize ? 1 : 0}:${opt(a.target)}:${opt(a.card)}`;
    case 'voxSkip':
      return 'vs';
    case 'reinforce':
      return `rf:${a.system}`;
    default: {
      const never: never = a;
      throw new Error(`unencodable action ${JSON.stringify(never)}`);
    }
  }
}
