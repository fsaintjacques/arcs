/** Guild card abilities: the ones the engine dispatches (court.ts POWER_STATUS). */
import { describe, expect, it } from 'vitest';
import {
  COURT_DECK,
  IMPLEMENTED_POWERS,
  POWER_STATUS,
  UNIMPLEMENTED_POWERS,
  cloneState,
  controlOf,
  courtCard,
  survivesOutrage,
  type Action,
} from '../src/engine';
import { AGGRESSION, CONSTRUCTION, actions, actor, apply, cardId, find, setHand, startGame } from './helpers';

/** The card id of a Court card by name. */
function card(name: string): number {
  const c = COURT_DECK.find((x) => x.name === name);
  if (!c) throw new Error(`no card ${name}`);
  return c.id;
}

/** Put `player` on turn holding `cards`, having led a card of `suit`. */
function turnHolding(seed: number, cards: string[], suit = CONSTRUCTION, number = 2) {
  const f = startGame(3, seed);
  const player = actor(f);
  f.s.playerStates[player].guildCards = cards.map(card);
  setHand(f, player, [cardId(suit, number)]);
  apply(f, { t: 'lead', card: cardId(suit, number) });
  return { f, player };
}

describe('power status bookkeeping', () => {
  it('classifies every card that carries an ability', () => {
    for (const c of COURT_DECK) {
      if (!c.power && !c.vox) continue;
      expect(POWER_STATUS[c.name], c.name).toBeDefined();
    }
  });

  it('names what is missing for anything not fully dispatched', () => {
    for (const [name, entry] of Object.entries(POWER_STATUS)) {
      if (entry.status === 'full') continue;
      expect(entry.missing, name).toBeTruthy();
    }
  });

  it('splits the deck into the implemented set and its complement', () => {
    expect(IMPLEMENTED_POWERS.size + UNIMPLEMENTED_POWERS.length).toBe(
      Object.keys(POWER_STATUS).length,
    );
    for (const name of IMPLEMENTED_POWERS) expect(UNIMPLEMENTED_POWERS).not.toContain(name);
  });
});

describe('Loyal cards', () => {
  it('let any resource be spent as their type', () => {
    const { f, player } = turnHolding(301, ['Loyal Keepers']);
    f.s.playerStates[player].resources.fill(null);
    f.s.playerStates[player].resources[0] = 'fuel';

    const as = actions(f).filter((a) => a.t === 'spendResourceAs');
    expect(as.some((a) => a.t === 'spendResourceAs' && a.as === 'relic')).toBe(true);

    // Spending Fuel as a Relic buys a Secure, which Fuel alone would not.
    apply(f, find<Extract<Action, { t: 'spendResourceAs' }>>(actions(f), (a) => a.t === 'spendResourceAs' && a.as === 'relic'));
    apply(f, { t: 'beginActions' });
    expect(f.s.turn!.freeActions.some((g) => g.includes('secure'))).toBe(true);
  });

  it('returns the real token to the supply, not the type it was spent as', () => {
    const { f, player } = turnHolding(302, ['Loyal Keepers']);
    f.s.playerStates[player].resources.fill(null);
    f.s.playerStates[player].resources[0] = 'fuel';
    const fuelBefore = f.s.supply.fuel;
    const relicBefore = f.s.supply.relic;

    apply(f, find<Extract<Action, { t: 'spendResourceAs' }>>(actions(f), (a) => a.t === 'spendResourceAs' && a.as === 'relic'));
    apply(f, { t: 'beginActions' });

    expect(f.s.supply.fuel).toBe(fuelBefore + 1);
    expect(f.s.supply.relic).toBe(relicBefore);
  });

  it('survive the Outrage discard of their own suit', () => {
    // Both are Relic cards, so Outraging Relic would discard both — but
    // "If you Provoke Outrage, keep this card" exempts the Loyal one.
    expect(courtCard(card('Loyal Keepers')).suit).toBe('relic');
    expect(courtCard(card('Relic Fence')).suit).toBe('relic');
    expect(survivesOutrage(card('Loyal Keepers'))).toBe(true);
    expect(survivesOutrage(card('Relic Fence'))).toBe(false);
  });
});

describe('Gatekeepers', () => {
  it('collects 2 extra battle dice in a gate but not on a planet', () => {
    const countDice = (holding: string[], onGate: boolean) => {
      const { f, player } = turnHolding(311, holding, AGGRESSION, 2);
      const rival = (player + 1) % 3;
      const system = f.s.systems.findIndex(
        (sys, i) => (f.v.systems[i].kind === 'gate') === onGate && !sys.outOfPlay,
      );
      f.s.systems[system].fresh[player] = 2;
      f.s.systems[system].fresh[rival] = 1;
      apply(f, { t: 'beginActions' });
      const max = Math.max(
        0,
        ...actions(f)
          .filter((a) => a.t === 'battle' && a.system === system)
          .map((a) => (a as Extract<Action, { t: 'battle' }>).assault),
      );
      return max;
    };
    expect(countDice(['Gatekeepers'], true)).toBe(4); // 2 ships + 2
    expect(countDice([], true)).toBe(2);
    expect(countDice(['Gatekeepers'], false)).toBe(2); // no bonus off a gate
  });

  it('places a ship in every in-play gate for its Prelude', () => {
    const { f, player } = turnHolding(312, ['Gatekeepers']);
    const gates = f.v.systems.filter((d) => d.kind === 'gate' && !f.s.systems[d.id].outOfPlay);
    const before = gates.map((d) => f.s.systems[d.id].fresh[player]);

    apply(f, find(actions(f), (a) => a.t === 'cardPrelude' && a.card === card('Gatekeepers')));

    gates.forEach((d, i) => expect(f.s.systems[d.id].fresh[player]).toBe(before[i] + 1));
    expect(f.s.playerStates[player].guildCards).not.toContain(card('Gatekeepers'));
  });
});

describe('Secret Order', () => {
  it('keeps the lead card number when declaring Keeper or Empath', () => {
    const { f } = turnHolding(321, ['Secret Order'], CONSTRUCTION, 5); // "5" = Keeper
    apply(f, { t: 'declareAmbition', ambition: 'keeper' });
    expect(f.s.round.leadNumber).toBe(5);
  });

  it('still zeroes the card for other ambitions', () => {
    const { f } = turnHolding(322, ['Secret Order'], CONSTRUCTION, 4); // "4" = Warlord
    apply(f, { t: 'declareAmbition', ambition: 'warlord' });
    expect(f.s.round.leadNumber).toBe(0);
  });

  it('and without it, Keeper zeroes the card as normal', () => {
    const { f } = turnHolding(323, [], CONSTRUCTION, 5);
    apply(f, { t: 'declareAmbition', ambition: 'keeper' });
    expect(f.s.round.leadNumber).toBe(0);
  });
});

describe('Sworn Guardians', () => {
  it('is the only thing a raider may take', () => {
    const { f, player } = turnHolding(331, [], AGGRESSION, 2);
    const victim = (player + 1) % 3;
    const vs = f.s.playerStates[victim];
    vs.guildCards = [card('Sworn Guardians'), card('Relic Fence')];
    vs.resources.fill(null);
    vs.resources[0] = 'fuel';

    const system = f.s.systems.findIndex((sys, i) => f.v.systems[i].kind === 'planet' && !sys.outOfPlay);
    f.s.systems[system].fresh[player] = 3;
    f.s.systems[system].fresh[victim] = 1;
    apply(f, { t: 'beginActions' });

    f.s.battle = {
      system,
      attacker: player,
      defender: victim,
      dice: { assault: 0, skirmish: 0, raid: 0 },
      selfHits: 0,
      intercept: 0,
      hits: 0,
      buildingHits: 0,
      keys: 3,
      interceptResolved: false,
    };
    f.s.phase = 'battleAssign';

    const raidable = actions(f).filter((a) => a.t === 'raidResource' || a.t === 'raidCard');
    expect(raidable).toHaveLength(1);
    expect(raidable[0]).toEqual({ t: 'raidCard', card: card('Sworn Guardians') });
  });
});

describe('new actions', () => {
  it('Mining Interest adds Manufacture (Build): gain 1 Material', () => {
    const { f, player } = turnHolding(341, ['Mining Interest'], CONSTRUCTION, 2);
    apply(f, { t: 'beginActions' });
    const act = find(actions(f), (a) => a.t === 'cardAction' && a.name === 'Manufacture');

    const before = f.s.playerStates[player].resources.filter((r) => r === 'material').length;
    const supply = f.s.supply.material;
    const pips = f.s.turn!.pipsLeft;
    apply(f, act);

    expect(f.s.playerStates[player].resources.filter((r) => r === 'material').length).toBe(before + 1);
    expect(f.s.supply.material).toBe(supply - 1);
    expect(f.s.turn!.pipsLeft).toBe(pips - 1); // paid for with a Build pip
  });

  it('is not offered from a suit that cannot buy the action it replaces', () => {
    const { f } = turnHolding(342, ['Mining Interest'], AGGRESSION, 2); // no Build pips
    apply(f, { t: 'beginActions' });
    expect(actions(f).some((a) => a.t === 'cardAction' && a.name === 'Manufacture')).toBe(false);
  });
});

describe('Prelude abilities', () => {
  it('Lattice Spies seizes the initiative without burning a card', () => {
    const f = startGame(3, 351);
    const leader = actor(f);
    setHand(f, leader, [cardId(CONSTRUCTION, 2)]);
    apply(f, { t: 'lead', card: cardId(CONSTRUCTION, 2) });
    apply(f, { t: 'beginActions' });
    while (f.s.turn) apply(f, { t: 'endTurn' });

    const follower = actor(f);
    f.s.playerStates[follower].guildCards = [card('Lattice Spies')];
    setHand(f, follower, [cardId(CONSTRUCTION, 6)]);
    apply(f, { t: 'follow', card: cardId(CONSTRUCTION, 6), mode: 'surpass' });

    apply(f, find(actions(f), (a) => a.t === 'cardPrelude' && a.card === card('Lattice Spies')));
    expect(f.s.round.seizedBy).toBe(follower);
    expect(f.s.playerStates[follower].hand).toHaveLength(0); // no card burned
  });

  it('Silver-Tongues steals a named resource from a named Rival', () => {
    const { f, player } = turnHolding(352, ['Silver-Tongues']);
    const victim = (player + 1) % 3;
    f.s.playerStates[victim].resources.fill(null);
    f.s.playerStates[victim].resources[0] = 'relic';
    f.s.playerStates[player].resources.fill(null);

    apply(f, find(actions(f), (a) => a.t === 'cardPrelude' && a.card === card('Silver-Tongues')));
    expect(f.s.playerStates[player].resources).toContain('relic');
    expect(f.s.playerStates[victim].resources.filter(Boolean)).toHaveLength(0);
  });

  it('Relic Fence trades a resource for a Relic and stays in play, once per turn', () => {
    const { f, player } = turnHolding(353, ['Relic Fence']);
    const p = f.s.playerStates[player];
    p.resources.fill(null);
    p.resources[0] = 'fuel';
    p.resources[1] = 'material';

    apply(f, find(actions(f), (a) => a.t === 'cardPrelude' && a.card === card('Relic Fence')));
    expect(p.resources).toContain('relic');
    expect(p.guildCards).toContain(card('Relic Fence')); // kept, not discarded
    // Once per turn.
    expect(actions(f).some((a) => a.t === 'cardPrelude' && a.card === card('Relic Fence'))).toBe(false);
  });

  it('Shipping Interest fills every empty slot with Fuel', () => {
    const { f, player } = turnHolding(354, ['Shipping Interest']);
    const p = f.s.playerStates[player];
    p.resources.fill(null);
    p.resources[0] = 'material';

    apply(f, find(actions(f), (a) => a.t === 'cardPrelude' && a.card === card('Shipping Interest')));
    expect(p.resources.filter((r) => r === 'fuel').length).toBeGreaterThan(0);
    expect(p.resources.filter((r) => r === null).length).toBe(
      p.resources.length - openCount(p.resources),
    );
  });

  it('"place 3 ships" only offers systems you control', () => {
    const { f, player } = turnHolding(355, ['Loyal Marines']);
    const offered = actions(f)
      .filter((a) => a.t === 'cardPrelude' && a.card === card('Loyal Marines'))
      .map((a) => (a as Extract<Action, { t: 'cardPrelude' }>).system!);
    expect(offered.length).toBeGreaterThan(0);
    for (const system of offered) expect(controlOf(f.s, system)).toBe(player);
  });

  it('cannot be used on a card secured in the same Prelude (p20)', () => {
    const { f, player } = turnHolding(356, ['Silver-Tongues']);
    const victim = (player + 1) % 3;
    f.s.playerStates[victim].resources[0] = 'relic';
    f.s.turn!.securedThisPrelude.push(card('Silver-Tongues'));
    expect(actions(f).some((a) => a.t === 'cardPrelude')).toBe(false);
  });
});

function openCount(resources: (string | null)[]): number {
  return resources.filter((r) => r !== null).length;
}

describe('determinized search safety', () => {
  it('a rollout never mutates the real turn state', () => {
    // Regression: `cardPreludesUsed` was shared by reference across clones, so
    // an MCTS rollout that used Relic Fence removed the option from the real game.
    const { f } = turnHolding(361, ['Relic Fence']);
    f.s.playerStates[f.s.turn!.player].resources[0] = 'fuel';
    const before = actions(f).length;

    const copy = cloneState(f.s);
    copy.turn!.cardPreludesUsed.push(card('Relic Fence'));
    copy.turn!.securedThisPrelude.push(1);
    copy.turn!.preludeSpent.push('fuel');

    expect(f.s.turn!.cardPreludesUsed).toHaveLength(0);
    expect(f.s.turn!.securedThisPrelude).toHaveLength(0);
    expect(actions(f)).toHaveLength(before);
  });
});
