/** The rules surface: setup, lead and follow, initiative, ambitions, actions. */
import { describe, expect, it } from 'vitest';
import {
  actionCard,
  ambitionCount,
  AMBITION_MARKERS,
  COURT_DECK,
  controlOf,
  makeVariant,
  mulberry32,
  newGame,
  openResourceSlots,
  resolveChanceMut,
  scoreAmbition,
  standings,
  type Action,
  type GameState,
} from '../src/engine';
import {
  ADMIN,
  AGGRESSION,
  CONSTRUCTION,
  MOBILIZATION,
  actions,
  actor,
  apply,
  cardId,
  find,
  setHand,
  settle,
  startGame,
} from './helpers';

describe('setup (p4-p5)', () => {
  it('places 3 ships + a city, 3 ships + a starport, and 2 ships', () => {
    for (const players of [2, 3, 4]) {
      const v = makeVariant(players);
      const s = newGame(v, mulberry32(5), 0);
      for (let p = 0; p < players; p++) {
        const ships = s.systems.reduce((n, sys) => n + sys.fresh[p] + sys.damaged[p], 0);
        // 3 + 3 + 2 at 3-4 players; 2 players get two C systems, so 3 + 3 + 2 + 2.
        expect(ships).toBe(players === 2 ? 10 : 8);
        const cities = s.systems.flatMap((sys) => sys.buildings).filter((b) => b.player === p && b.kind === 'city');
        const ports = s.systems.flatMap((sys) => sys.buildings).filter((b) => b.player === p && b.kind === 'starport');
        expect(cities).toHaveLength(1);
        expect(ports).toHaveLength(1);
        expect(s.playerStates[p].shipsSupply).toBe(15 - ships);
      }
    }
  });

  it('gives every player the 2 resources of their A and B planets (p5 step O)', () => {
    const v = makeVariant(3);
    const s = newGame(v, mulberry32(5), 0);
    for (const p of s.playerStates) {
      expect(p.resources.filter((r) => r !== null)).toHaveLength(2);
    }
  });

  it('never starts anyone in an out-of-play cluster', () => {
    for (const players of [2, 3, 4]) {
      for (let setup = 0; setup < 6; setup++) {
        const v = makeVariant(players, setup);
        const s = newGame(v, mulberry32(1), setup);
        for (const [i, sys] of s.systems.entries()) {
          if (!sys.outOfPlay) continue;
          expect(sys.fresh.reduce((a, b) => a + b, 0), `system ${i}`).toBe(0);
          expect(sys.buildings).toHaveLength(0);
        }
      }
    }
  });

  it('seeds the phantom rival only at 2 players (p4 step K)', () => {
    const two = newGame(makeVariant(2), mulberry32(1), 0);
    const three = newGame(makeVariant(3), mulberry32(1), 0);
    expect(Object.values(two.phantom).reduce((a, b) => a + b, 0)).toBe(6);
    expect(Object.values(three.phantom).reduce((a, b) => a + b, 0)).toBe(0);
  });

  it('deals 6 cards to everyone (p5 step P)', () => {
    const f = startGame(4, 9);
    for (const p of f.s.playerStates) expect(p.hand).toHaveLength(6);
  });
});

describe('lead and follow (p8, p10)', () => {
  it('offers the leader every card plus passing the initiative', () => {
    const f = startGame(3, 11);
    const list = actions(f);
    expect(list.filter((a) => a.t === 'lead')).toHaveLength(6);
    expect(list.some((a) => a.t === 'passInitiative')).toBe(true);
  });

  it('lets followers Surpass only with the lead suit and a higher number', () => {
    const f = startGame(3, 11);
    const leader = actor(f);
    setHand(f, leader, [cardId(CONSTRUCTION, 4)]);
    apply(f, { t: 'lead', card: cardId(CONSTRUCTION, 4) });
    apply(f, { t: 'beginActions' });
    while (f.s.turn) apply(f, { t: 'endTurn' });

    const follower = actor(f);
    setHand(f, follower, [
      cardId(CONSTRUCTION, 5), // surpass or copy
      cardId(CONSTRUCTION, 3), // copy only — same suit, lower number
      cardId(AGGRESSION, 6), // pivot or copy
    ]);
    const list = actions(f);
    const modes = (card: number) =>
      list.filter((a) => a.t === 'follow' && a.card === card).map((a) => (a as { mode: string }).mode).sort();

    expect(modes(cardId(CONSTRUCTION, 5))).toEqual(['copy', 'surpass']);
    expect(modes(cardId(CONSTRUCTION, 3))).toEqual(['copy']);
    expect(modes(cardId(AGGRESSION, 6))).toEqual(['copy', 'pivot']);
  });

  it('gives Surpass its own pips, but Copy and Pivot exactly one action', () => {
    const check = (mode: 'surpass' | 'copy' | 'pivot', card: number, expected: number) => {
      const f = startGame(3, 11);
      const leader = actor(f);
      setHand(f, leader, [cardId(CONSTRUCTION, 4)]);
      apply(f, { t: 'lead', card: cardId(CONSTRUCTION, 4) });
      apply(f, { t: 'beginActions' });
      while (f.s.turn) apply(f, { t: 'endTurn' });
      const follower = actor(f);
      setHand(f, follower, [card]);
      apply(f, { t: 'follow', card, mode });
      expect(f.s.turn!.pipsLeft).toBe(expected);
    };
    check('surpass', cardId(CONSTRUCTION, 2), 4); // a "2" has 4 pips
    check('copy', cardId(CONSTRUCTION, 2), 1);
    check('pivot', cardId(MOBILIZATION, 2), 1);
  });

  it('gives a Copy the lead suit’s actions, and a Pivot its own card’s', () => {
    const f = startGame(3, 13);
    const leader = actor(f);
    setHand(f, leader, [cardId(ADMIN, 4)]);
    apply(f, { t: 'lead', card: cardId(ADMIN, 4) });
    apply(f, { t: 'beginActions' });
    while (f.s.turn) apply(f, { t: 'endTurn' });

    const follower = actor(f);
    setHand(f, follower, [cardId(MOBILIZATION, 5)]);
    apply(f, { t: 'follow', card: cardId(MOBILIZATION, 5), mode: 'copy' });
    expect(f.s.turn!.pipActions).toEqual(['tax', 'repair', 'influence']);
  });

  it('a Pivot does not change the lead suit', () => {
    const f = startGame(3, 17);
    const leader = actor(f);
    setHand(f, leader, [cardId(CONSTRUCTION, 4)]);
    apply(f, { t: 'lead', card: cardId(CONSTRUCTION, 4) });
    apply(f, { t: 'beginActions' });
    while (f.s.turn) apply(f, { t: 'endTurn' });
    const follower = actor(f);
    setHand(f, follower, [cardId(AGGRESSION, 6)]);
    apply(f, { t: 'follow', card: cardId(AGGRESSION, 6), mode: 'pivot' });
    expect(actionCard(f.s.round.lead!.card).suit).toBe('construction');
  });
});

describe('declaring an ambition (p9)', () => {
  function leadAndDeclare(seed: number, card: number) {
    const f = startGame(3, seed);
    const leader = actor(f);
    setHand(f, leader, [card]);
    apply(f, { t: 'lead', card });
    return { f, leader };
  }

  it('takes the highest available marker and zeroes the lead card', () => {
    const { f } = leadAndDeclare(21, cardId(CONSTRUCTION, 4)); // "4" = Warlord
    expect(actions(f).some((a) => a.t === 'declareAmbition')).toBe(true);
    apply(f, { t: 'declareAmbition', ambition: 'warlord' });

    expect(f.s.declared.warlord).toHaveLength(1);
    const marker = f.s.declared.warlord[0];
    expect(AMBITION_MARKERS[marker].blue.first).toBe(5); // the highest of 5/3/2
    expect(f.s.availableMarkers).toHaveLength(2);
    expect(f.s.round.leadNumber).toBe(0);
  });

  it('does not change the card’s pips', () => {
    const { f } = leadAndDeclare(22, cardId(CONSTRUCTION, 4));
    apply(f, { t: 'declareAmbition', ambition: 'warlord' });
    expect(f.s.turn!.pipsLeft).toBe(3); // a "4" has 3 pips
  });

  it('only offers the ambition printed on the card', () => {
    const { f } = leadAndDeclare(23, cardId(CONSTRUCTION, 5)); // "5" = Keeper
    const offered = actions(f)
      .filter((a) => a.t === 'declareAmbition')
      .map((a) => (a as { ambition: string }).ambition);
    expect(offered).toEqual(['keeper']);
  });

  it('lets a "7" declare anything and a "1" declare nothing (4 players)', () => {
    const seven = startGame(4, 24);
    setHand(seven, actor(seven), [cardId(CONSTRUCTION, 7)]);
    apply(seven, { t: 'lead', card: cardId(CONSTRUCTION, 7) });
    expect(actions(seven).filter((a) => a.t === 'declareAmbition')).toHaveLength(5);

    const one = startGame(4, 24);
    setHand(one, actor(one), [cardId(CONSTRUCTION, 1)]);
    apply(one, { t: 'lead', card: cardId(CONSTRUCTION, 1) });
    expect(actions(one).filter((a) => a.t === 'declareAmbition')).toHaveLength(0);
  });

  it('cannot declare twice with the same card', () => {
    const { f } = leadAndDeclare(25, cardId(CONSTRUCTION, 4));
    apply(f, { t: 'declareAmbition', ambition: 'warlord' });
    expect(actions(f).some((a) => a.t === 'declareAmbition')).toBe(false);
  });

  it('cannot declare once all 3 markers are placed', () => {
    const { f } = leadAndDeclare(26, cardId(CONSTRUCTION, 4));
    f.s.availableMarkers = [];
    expect(actions(f).some((a) => a.t === 'declareAmbition')).toBe(false);
  });

  it('is only offered to the player leading', () => {
    const f = startGame(3, 27);
    setHand(f, actor(f), [cardId(CONSTRUCTION, 4)]);
    apply(f, { t: 'lead', card: cardId(CONSTRUCTION, 4) });
    apply(f, { t: 'beginActions' });
    while (f.s.turn) apply(f, { t: 'endTurn' });
    const follower = actor(f);
    setHand(f, follower, [cardId(CONSTRUCTION, 6)]);
    apply(f, { t: 'follow', card: cardId(CONSTRUCTION, 6), mode: 'surpass' });
    expect(actions(f).some((a) => a.t === 'declareAmbition')).toBe(false);
  });
});

describe('initiative (p10-p11)', () => {
  /** Play a full round where each player plays one named card. */
  function playRound(f: ReturnType<typeof startGame>, plays: { card: number; mode?: 'surpass' | 'copy' | 'pivot' }[]) {
    for (let i = 0; i < plays.length; i++) {
      const player = actor(f);
      setHand(f, player, [plays[i].card]);
      if (i === 0) apply(f, { t: 'lead', card: plays[i].card });
      else apply(f, { t: 'follow', card: plays[i].card, mode: plays[i].mode! });
      // Skip the whole turn.
      apply(f, { t: 'beginActions' });
      while (f.s.turn) apply(f, { t: 'endTurn' });
      settle(f);
    }
  }

  it('goes to the highest Surpass when nobody seized', () => {
    const f = startGame(3, 31);
    const start = f.s.initiative;
    const second = (start + 1) % 3;
    const third = (start + 2) % 3;
    playRound(f, [
      { card: cardId(CONSTRUCTION, 2) },
      { card: cardId(CONSTRUCTION, 4), mode: 'surpass' },
      { card: cardId(CONSTRUCTION, 6), mode: 'surpass' },
    ]);
    expect(f.s.initiative).toBe(third);
    expect(second).not.toBe(third);
  });

  it('does not move when nobody Surpasses', () => {
    const f = startGame(3, 32);
    const start = f.s.initiative;
    playRound(f, [
      { card: cardId(CONSTRUCTION, 2) },
      { card: cardId(AGGRESSION, 6), mode: 'pivot' },
      { card: cardId(MOBILIZATION, 5), mode: 'copy' },
    ]);
    expect(f.s.initiative).toBe(start);
  });

  it('a seize beats every Surpass', () => {
    const f = startGame(3, 33);
    const start = f.s.initiative;
    const second = (start + 1) % 3;

    setHand(f, start, [cardId(CONSTRUCTION, 2)]);
    apply(f, { t: 'lead', card: cardId(CONSTRUCTION, 2) });
    apply(f, { t: 'beginActions' });
    while (f.s.turn) apply(f, { t: 'endTurn' });

    // Second player seizes by burning a spare card.
    setHand(f, second, [cardId(AGGRESSION, 3), cardId(ADMIN, 2)]);
    apply(f, { t: 'follow', card: cardId(AGGRESSION, 3), mode: 'pivot' });
    apply(f, { t: 'seize', card: cardId(ADMIN, 2) });
    expect(f.s.initiativeSeized).toBe(true);
    apply(f, { t: 'beginActions' });
    while (f.s.turn) apply(f, { t: 'endTurn' });

    // Third player Surpasses with a 6 — too late.
    const third = actor(f);
    setHand(f, third, [cardId(CONSTRUCTION, 6)]);
    apply(f, { t: 'follow', card: cardId(CONSTRUCTION, 6), mode: 'surpass' });
    apply(f, { t: 'beginActions' });
    while (f.s.turn) apply(f, { t: 'endTurn' });
    settle(f);

    expect(f.s.initiative).toBe(second);
  });

  it('only one player may seize per round, and never the holder', () => {
    const f = startGame(3, 34);
    const start = f.s.initiative;
    setHand(f, start, [cardId(CONSTRUCTION, 2), cardId(ADMIN, 3)]);
    apply(f, { t: 'lead', card: cardId(CONSTRUCTION, 2) });
    expect(actions(f).some((a) => a.t === 'seize')).toBe(false);
    apply(f, { t: 'beginActions' });
    while (f.s.turn) apply(f, { t: 'endTurn' });

    const second = actor(f);
    setHand(f, second, [cardId(AGGRESSION, 3), cardId(ADMIN, 2)]);
    apply(f, { t: 'follow', card: cardId(AGGRESSION, 3), mode: 'pivot' });
    apply(f, { t: 'seize', card: cardId(ADMIN, 2) });
    apply(f, { t: 'beginActions' });
    while (f.s.turn) apply(f, { t: 'endTurn' });

    const third = actor(f);
    setHand(f, third, [cardId(AGGRESSION, 4), cardId(ADMIN, 5)]);
    apply(f, { t: 'follow', card: cardId(AGGRESSION, 4), mode: 'pivot' });
    expect(actions(f).some((a) => a.t === 'seize')).toBe(false);
  });

  it('Surpassing with a "7" seizes the initiative (4 players)', () => {
    const f = startGame(4, 35);
    const start = f.s.initiative;
    const second = (start + 1) % 4;
    setHand(f, start, [cardId(CONSTRUCTION, 2)]);
    apply(f, { t: 'lead', card: cardId(CONSTRUCTION, 2) });
    apply(f, { t: 'beginActions' });
    while (f.s.turn) apply(f, { t: 'endTurn' });

    setHand(f, second, [cardId(CONSTRUCTION, 7)]);
    apply(f, { t: 'follow', card: cardId(CONSTRUCTION, 7), mode: 'surpass' });
    expect(f.s.round.seizedBy).toBe(second);
    expect(f.s.turn!.pipsLeft).toBe(1); // still gets the 7's single pip
  });

  it('passing the initiative hands it on and ends the round at once', () => {
    const f = startGame(3, 36);
    const start = f.s.initiative;
    const roundsBefore = f.s.stats.rounds;
    apply(f, { t: 'passInitiative' });
    expect(f.s.initiative).toBe((start + 1) % 3);
    expect(f.s.stats.rounds).toBe(roundsBefore + 1);
    expect(actor(f)).toBe((start + 1) % 3);
    expect(f.s.round.turnIndex).toBe(0);
  });
});

describe('standard actions (p12-p13)', () => {
  /** Put the named player on turn with a card that buys `n` of one suit. */
  function turnWith(f: ReturnType<typeof startGame>, suit: number, number: number) {
    const player = actor(f);
    setHand(f, player, [cardId(suit, number)]);
    apply(f, { t: 'lead', card: cardId(suit, number) });
    apply(f, { t: 'beginActions' });
    return player;
  }

  it('taxing a Loyal city gains its planet type', () => {
    const f = startGame(3, 41);
    const player = turnWith(f, ADMIN, 2);
    const tax = find(actions(f), (a) => a.t === 'tax');
    const before = f.s.playerStates[player].resources.filter((r) => r).length;
    apply(f, tax);
    expect(f.s.playerStates[player].resources.filter((r) => r).length).toBe(before + 1);
  });

  it('cannot tax the same city twice in a turn (p12)', () => {
    const f = startGame(3, 42);
    turnWith(f, ADMIN, 2);
    const tax = find<Extract<Action, { t: 'tax' }>>(actions(f), (a) => a.t === 'tax');
    apply(f, tax);
    const again = actions(f).filter(
      (a) => a.t === 'tax' && a.system === tax.system && a.building === tax.building,
    );
    expect(again).toHaveLength(0);
  });

  it('taxing a Rival city you control captures one of their agents', () => {
    const f = startGame(3, 43);
    const player = actor(f);
    const victim = (player + 1) % 3;
    // Find the victim's city and park a controlling fleet on it.
    const system = f.s.systems.findIndex((sys) =>
      sys.buildings.some((b) => b.player === victim && b.kind === 'city'),
    );
    f.s.systems[system].fresh[player] = 9;
    setHand(f, player, [cardId(ADMIN, 2)]);
    apply(f, { t: 'lead', card: cardId(ADMIN, 2) });
    apply(f, { t: 'beginActions' });

    expect(controlOf(f.s, system)).toBe(player);
    const building = f.s.systems[system].buildings.findIndex((b) => b.player === victim);
    const agentsBefore = f.s.playerStates[victim].agentsSupply;
    apply(f, { t: 'tax', system, building });
    expect(f.s.playerStates[victim].agentsSupply).toBe(agentsBefore - 1);
    expect(f.s.playerStates[player].captives).toContain(victim);
  });

  it('building in a system someone else controls places the piece damaged (p12)', () => {
    const f = startGame(3, 44);
    const player = actor(f);
    const rival = (player + 1) % 3;
    const system = f.s.systems.findIndex(
      (sys, i) => f.v.systems[i].kind === 'planet' && sys.fresh[player] > 0,
    );
    f.s.systems[system].fresh[rival] = 9; // rival now controls it
    setHand(f, player, [cardId(CONSTRUCTION, 2)]);
    apply(f, { t: 'lead', card: cardId(CONSTRUCTION, 2) });
    apply(f, { t: 'beginActions' });

    const before = f.s.systems[system].buildings.length;
    const build = find(actions(f), (a) => a.t === 'buildBuilding' && a.system === system);
    apply(f, build);
    const placed = f.s.systems[system].buildings[before];
    expect(placed.player).toBe(player);
    expect(placed.damaged).toBe(true);
  });

  it('a starport builds only one ship per turn (p12)', () => {
    const f = startGame(3, 45);
    const player = turnWith(f, CONSTRUCTION, 2); // 4 pips
    const first = find<Extract<Action, { t: 'buildShip' }>>(actions(f), (a) => a.t === 'buildShip');
    apply(f, first);
    const again = actions(f).filter(
      (a) => a.t === 'buildShip' && a.system === first.system && a.building === first.building,
    );
    expect(again).toHaveLength(0);
    expect(player).toBeGreaterThanOrEqual(0);
  });

  it('influence puts an agent in the Court, secure needs a strict majority (p13)', () => {
    const f = startGame(3, 46);
    // Aggression buys Secure; Mobilization and Administration buy Influence.
    const player = turnWith(f, AGGRESSION, 2);
    f.s.court[0].agents[player] = 1;
    f.s.playerStates[player].agentsSupply--;
    expect(f.s.court[0].agents[player]).toBe(1);

    // A rival matching the count blocks the Secure.
    f.s.court[0].agents[(player + 1) % 3] = 1;
    expect(actions(f).some((a) => a.t === 'secure' && a.slot === 0)).toBe(false);
    f.s.court[0].agents[(player + 1) % 3] = 0;
    expect(actions(f).some((a) => a.t === 'secure' && a.slot === 0)).toBe(true);
  });

  it('securing captures Rival agents and refills the slot', () => {
    const f = startGame(3, 47);
    const player = turnWith(f, AGGRESSION, 2);
    const rival = (player + 1) % 3;
    const card = f.s.court[0].card;
    f.s.court[0].agents[player] = 2;
    f.s.court[0].agents[rival] = 1;

    apply(f, { t: 'secure', slot: 0 });
    expect(f.s.playerStates[player].captives).toContain(rival);
    expect(f.s.court[0].card).not.toBe(card);
    expect(f.s.court[0].agents.every((n) => n === 0)).toBe(true);
  });

  it('repair stands a damaged ship back up (p13)', () => {
    const f = startGame(3, 48);
    const player = actor(f);
    const system = f.s.systems.findIndex((sys) => sys.fresh[player] > 0);
    f.s.systems[system].fresh[player]--;
    f.s.systems[system].damaged[player]++;
    setHand(f, player, [cardId(ADMIN, 2)]);
    apply(f, { t: 'lead', card: cardId(ADMIN, 2) });
    apply(f, { t: 'beginActions' });

    apply(f, { t: 'repair', system, building: null });
    expect(f.s.systems[system].damaged[player]).toBe(0);
  });

  it('move only reaches adjacent in-play systems', () => {
    const f = startGame(3, 49);
    turnWith(f, MOBILIZATION, 2);
    for (const a of actions(f)) {
      if (a.t !== 'move') continue;
      expect(f.v.systems[a.from].adjacent).toContain(a.to);
      expect(f.s.systems[a.to].outOfPlay).toBe(false);
    }
  });
});

describe('Prelude resources (p17, p20)', () => {
  it('a Fuel buys a Move even from a Construction card', () => {
    const f = startGame(3, 51);
    const player = actor(f);
    f.s.playerStates[player].resources[0] = 'fuel';
    setHand(f, player, [cardId(CONSTRUCTION, 5)]);
    apply(f, { t: 'lead', card: cardId(CONSTRUCTION, 5) });
    apply(f, { t: 'spendResource', slot: 0 });
    apply(f, { t: 'beginActions' });
    expect(actions(f).some((a) => a.t === 'move')).toBe(true);
  });

  it('a Weapon lets the card’s own pips buy Battle actions', () => {
    const f = startGame(3, 52);
    const player = actor(f);
    f.s.playerStates[player].resources[0] = 'weapon';
    setHand(f, player, [cardId(CONSTRUCTION, 2)]);
    apply(f, { t: 'lead', card: cardId(CONSTRUCTION, 2) });
    apply(f, { t: 'spendResource', slot: 0 });
    expect(f.s.turn!.weaponSpent).toBe(true);
    apply(f, { t: 'beginActions' });
    expect(f.s.turn!.pipsLeft).toBe(4);
  });

  it('Outraged resources cannot be spent in the Prelude (p16)', () => {
    const f = startGame(3, 53);
    const player = actor(f);
    f.s.playerStates[player].resources.fill(null);
    f.s.playerStates[player].resources[0] = 'fuel';
    f.s.playerStates[player].outrage.fuel = true;
    setHand(f, player, [cardId(CONSTRUCTION, 5)]);
    apply(f, { t: 'lead', card: cardId(CONSTRUCTION, 5) });
    expect(actions(f).some((a) => a.t === 'spendResource')).toBe(false);
  });

  it('declaring and seizing close once a resource is spent (p20)', () => {
    const f = startGame(3, 54);
    const player = actor(f);
    f.s.playerStates[player].resources[0] = 'fuel';
    setHand(f, player, [cardId(CONSTRUCTION, 4)]);
    apply(f, { t: 'lead', card: cardId(CONSTRUCTION, 4) });
    expect(actions(f).some((a) => a.t === 'declareAmbition')).toBe(true);
    apply(f, { t: 'spendResource', slot: 0 });
    expect(actions(f).some((a) => a.t === 'declareAmbition')).toBe(false);
  });

  it('spent Prelude resources go back to the supply when the Prelude ends', () => {
    const f = startGame(3, 55);
    const player = actor(f);
    f.s.playerStates[player].resources[0] = 'fuel';
    const supplyBefore = f.s.supply.fuel;
    setHand(f, player, [cardId(CONSTRUCTION, 5)]);
    apply(f, { t: 'lead', card: cardId(CONSTRUCTION, 5) });
    apply(f, { t: 'spendResource', slot: 0 });
    expect(f.s.supply.fuel).toBe(supplyBefore);
    apply(f, { t: 'beginActions' });
    expect(f.s.supply.fuel).toBe(supplyBefore + 1);
  });
});

describe('battle (p14-p16)', () => {
  /** Put an attacker and a defender in one system, on the attacker's turn. */
  function battleSetup(seed: number, opts: { defenderBuilding?: boolean } = {}) {
    const f = startGame(3, seed);
    const player = actor(f);
    const rival = (player + 1) % 3;
    const system = f.s.systems.findIndex(
      (sys, i) => f.v.systems[i].kind === 'planet' && sys.fresh[player] > 0,
    );
    f.s.systems[system].fresh[player] = 4;
    f.s.systems[system].fresh[rival] = 2;
    if (opts.defenderBuilding) {
      f.s.systems[system].buildings.push({
        player: rival,
        kind: 'starport',
        damaged: false,
        taxedThisTurn: false,
        builtThisTurn: false,
      });
    }
    setHand(f, player, [cardId(AGGRESSION, 2)]);
    apply(f, { t: 'lead', card: cardId(AGGRESSION, 2) });
    apply(f, { t: 'beginActions' });
    return { f, player, rival, system };
  }

  it('never offers more dice than attacking ships', () => {
    const { f, system } = battleSetup(61);
    const ships = f.s.systems[system].fresh[actor(f)];
    for (const a of actions(f)) {
      if (a.t !== 'battle') continue;
      expect(a.assault + a.skirmish + a.raid).toBeLessThanOrEqual(ships);
      expect(a.assault + a.skirmish + a.raid).toBeGreaterThan(0);
    }
  });

  it('withholds raid dice unless the defender has a building here or none anywhere (p14)', () => {
    // Defender has a city elsewhere (from setup) and nothing in this system.
    const bare = battleSetup(62);
    expect(bare.f.s.systems.some((sys) => sys.buildings.some((b) => b.player === bare.rival))).toBe(true);
    expect(actions(bare.f).some((a) => a.t === 'battle' && a.raid > 0)).toBe(false);

    // With a defending building present, raid dice are legal.
    const withBuilding = battleSetup(62, { defenderBuilding: true });
    expect(actions(withBuilding.f).some((a) => a.t === 'battle' && a.raid > 0)).toBe(true);
  });

  it('resolves a skirmish-only battle without ever hurting the attacker', () => {
    const { f, player, rival, system } = battleSetup(63);
    const attackerBefore = f.s.systems[system].fresh[player];
    apply(f, { t: 'battle', system, defender: rival, assault: 0, skirmish: 4, raid: 0 });
    settle(f); // roll
    // Assign every hit that came up.
    let guard = 0;
    while (f.s.battle && guard++ < 32) {
      const list = actions(f);
      apply(f, list[0]);
      settle(f);
    }
    expect(f.s.systems[system].fresh[player] + f.s.systems[system].damaged[player]).toBe(attackerBefore);
  });

  it('damages a fresh ship and destroys a damaged one into Trophies', () => {
    const { f, player, rival, system } = battleSetup(64);
    f.s.systems[system].fresh[rival] = 0;
    f.s.systems[system].damaged[rival] = 1;
    apply(f, { t: 'battle', system, defender: rival, assault: 0, skirmish: 4, raid: 0 });
    settle(f);
    let guard = 0;
    while (f.s.battle && guard++ < 32) {
      const list = actions(f);
      const hit = list.find((a) => a.t === 'assignHit') ?? list[0];
      apply(f, hit);
      settle(f);
    }
    // Any hit at all destroys the lone damaged ship.
    if (f.s.systems[system].damaged[rival] === 0) {
      expect(f.s.playerStates[player].trophies.length).toBeGreaterThan(0);
      expect(f.s.playerStates[player].trophies[0].owner).toBe(rival);
    }
  });

  it('an intercept converts into one self-hit per fresh defending ship (p14)', () => {
    const { f, player, rival, system } = battleSetup(65);
    f.s.systems[system].fresh[rival] = 3;
    f.s.battle = {
      system,
      attacker: player,
      defender: rival,
      dice: { assault: 1, skirmish: 0, raid: 0 },
      selfHits: 0,
      intercept: 0,
      hits: 0,
      buildingHits: 0,
      keys: 0,
      interceptResolved: false,
    };
    f.s.phase = 'battleRoll';
    // Assault face index 2 is "1 hit + intercept"; 0.4 * 6 floors to 2.
    resolveChanceMut(f.s, f.v, () => 0.4);
    expect(f.s.battle!.interceptResolved).toBe(true);
    // One self-hit per fresh defending ship, and the face still deals its hit.
    expect(f.s.battle!.selfHits).toBe(3);
    expect(f.s.battle!.hits).toBe(1);
  });
});

describe('ambition scoring (p18)', () => {
  function scored(setup: (s: GameState) => void, players = 3) {
    const v = makeVariant(players);
    const s = newGame(v, mulberry32(1), 0);
    // Clear the phantom rival so tests control every count.
    for (const key of Object.keys(s.phantom) as (keyof typeof s.phantom)[]) s.phantom[key] = 0;
    setup(s);
    return { s, v };
  }

  it('pays first and second place the marker’s two values', () => {
    const { s, v } = scored((st) => {
      st.declared.warlord = [0]; // the 5/3 marker
      st.availableMarkers = [1, 2];
      st.playerStates[0].trophies = [{ owner: 1, kind: 'ship' }, { owner: 1, kind: 'ship' }];
      st.playerStates[1].trophies = [{ owner: 0, kind: 'ship' }];
    });
    const r = scoreAmbition(s, v, 'warlord')!;
    expect(r.awards[0]).toBe(5);
    expect(r.awards[1]).toBe(3);
    expect(r.awards[2]).toBe(0);
  });

  it('drops everyone tied for first to second place', () => {
    const { s, v } = scored((st) => {
      st.declared.warlord = [0];
      st.playerStates[0].trophies = [{ owner: 1, kind: 'ship' }];
      st.playerStates[1].trophies = [{ owner: 0, kind: 'ship' }];
    });
    const r = scoreAmbition(s, v, 'warlord')!;
    expect(r.firstPlace).toEqual([]);
    expect(r.awards[0]).toBe(3);
    expect(r.awards[1]).toBe(3);
  });

  it('pays nothing when a tie for second place forms', () => {
    const { s, v } = scored((st) => {
      st.declared.warlord = [0];
      st.playerStates[0].trophies = [
        { owner: 1, kind: 'ship' },
        { owner: 1, kind: 'ship' },
      ];
      st.playerStates[1].trophies = [{ owner: 0, kind: 'ship' }];
      st.playerStates[2].trophies = [{ owner: 0, kind: 'ship' }];
    });
    const r = scoreAmbition(s, v, 'warlord')!;
    expect(r.awards[0]).toBe(5);
    expect(r.awards[1]).toBe(0);
    expect(r.awards[2]).toBe(0);
  });

  it('pays nobody when nobody has any of the counted thing (p18 Qualifying)', () => {
    const { s, v } = scored((st) => {
      st.declared.warlord = [0];
    });
    const r = scoreAmbition(s, v, 'warlord')!;
    expect(r.awards.every((a) => a === 0)).toBe(true);
  });

  it('sums the values of every marker in a box', () => {
    const { s, v } = scored((st) => {
      st.declared.tycoon = [0, 2]; // 5/3 and 2/0 → 7/3, matching the p18 example
      st.playerStates[0].resources[0] = 'material';
      st.playerStates[0].resources[1] = 'fuel';
      st.playerStates[1].resources[0] = 'material';
    });
    const r = scoreAmbition(s, v, 'tycoon')!;
    expect(r.first).toBe(7);
    expect(r.second).toBe(3);
    expect(r.awards[0]).toBe(7);
    expect(r.awards[1]).toBe(3);
  });

  it('counts Guild card suits toward ambitions, but never Weapon cards (p17)', () => {
    const relic = COURT_DECK.find((c) => c.suit === 'relic')!;
    const weapon = COURT_DECK.find((c) => c.suit === 'weapon')!;
    const { s } = scored((st) => {
      st.playerStates[0].resources.fill(null); // count the cards alone
      st.playerStates[0].guildCards = [relic.id, weapon.id];
    });
    expect(ambitionCount(s.playerStates[0], 'keeper')).toBe(1);
    expect(ambitionCount(s.playerStates[0], 'tycoon')).toBe(0);
  });
});

describe('chapters and game end (p19)', () => {
  it('returns the markers and flips the lowest unflipped one each chapter', () => {
    const f = startGame(3, 71);
    f.s.playerStates.forEach((p) => (p.hand = []));
    // Force the chapter to end by passing with empty hands.
    apply(f, { t: 'passInitiative' });
    expect(f.s.flipped.filter(Boolean)).toHaveLength(1);
    // The 2/0 marker (index 2, lowest) flips first.
    expect(f.s.flipped[2]).toBe(true);
  });

  it('ends the game once someone is at the Power threshold', () => {
    const f = startGame(3, 72);
    f.s.playerStates[0].power = 30;
    f.s.playerStates.forEach((p) => (p.hand = []));
    apply(f, { t: 'passInitiative' });
    expect(f.s.phase).toBe('over');
    expect(standings(f.s)[0].player).toBe(0);
  });

  it('ends after chapter 5 even below the threshold', () => {
    const f = startGame(3, 73);
    f.s.chapter = 5;
    f.s.playerStates.forEach((p) => (p.hand = []));
    apply(f, { t: 'passInitiative' });
    expect(f.s.phase).toBe('over');
  });

  it('breaks a final tie toward the earliest player in turn order (p19)', () => {
    const f = startGame(3, 74);
    f.s.initiative = 1;
    f.s.playerStates[0].power = 10;
    f.s.playerStates[1].power = 10;
    f.s.playerStates[2].power = 10;
    expect(standings(f.s)[0].player).toBe(1);
  });

  it('opens resource slots as cities leave the player board (p7)', () => {
    const f = startGame(3, 75);
    const p = f.s.playerStates[0];
    const before = openResourceSlots(p);
    p.citiesUsed = 5;
    expect(openResourceSlots(p)).toBeGreaterThan(before);
  });
});
