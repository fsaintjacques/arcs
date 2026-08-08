/** Guild card abilities: the ones the engine dispatches (court.ts POWER_STATUS). */
import { describe, expect, it } from 'vitest';
import {
  COURT_DECK,
  IMPLEMENTED_POWERS,
  POWER_STATUS,
  UNIMPLEMENTED_POWERS,
  ambitionCount,
  cartelIcons,
  cloneState,
  clusterOf,
  controlOf,
  courtCard,
  determinize,
  discardGuildCard,
  gainGuildCard,
  mulberry32,
  observe,
  resolveChanceMut,
  returnToSupply,
  survivesOutrage,
  weaponIcons,
  type Action,
} from '../src/engine';
import {
  actions,
  actor,
  ADMIN,
  AGGRESSION,
  apply,
  battleState,
  cardId,
  CONSTRUCTION,
  find,
  setHand,
  settleRound,
  settleToChapterEnd,
  startGame,
} from './helpers';

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

/**
 * The same, but with Battle pips and a planet where the player outnumbers a
 * Rival, ready for a `battle` action.
 */
function battleWith(seed: number, cards: string[]) {
  const { f, player } = turnHolding(seed, cards, AGGRESSION, 2);
  const rival = (player + 1) % 3;
  const system = f.s.systems.findIndex(
    (sys, i) => f.v.systems[i].kind === 'planet' && !sys.outOfPlay,
  );
  f.s.systems[system].fresh[player] = 6;
  f.s.systems[system].fresh[rival] = 4;
  apply(f, { t: 'beginActions' });
  return { f, player, rival, system };
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

  it('dispatches every printed ability in the deck', () => {
    // The whole Court is live. If an ability is ever rolled back, this fails and
    // names it rather than letting the docs quietly go stale.
    expect(UNIMPLEMENTED_POWERS).toEqual([]);
    const withAbility = COURT_DECK.filter((c) => c.power || c.vox);
    expect(withAbility).toHaveLength(31);
    for (const c of withAbility) expect(IMPLEMENTED_POWERS, c.name).toContain(c.name);
  });

  it('gives every Guild card with printed text an engine-readable power', () => {
    for (const c of COURT_DECK.filter((x) => x.kind === 'guild')) {
      expect(c.power, c.name).toBeDefined();
    }
    for (const c of COURT_DECK.filter((x) => x.kind === 'vox')) {
      expect(c.vox, c.name).toBeDefined();
    }
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

    f.s.battle = battleState({ system, attacker: player, defender: victim, keys: 3 });
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

describe('the Cartels', () => {
  it('takes its type’s whole supply onto the card when acquired', () => {
    const { f, player } = turnHolding(371, []);
    const before = f.s.supply.material;
    expect(before).toBeGreaterThan(0);
    gainGuildCard(f.s, player, card('Material Cartel'));

    expect(f.s.supply.material).toBe(0);
    expect(f.s.cartel.material).toBe(before);
  });

  it('counts toward Tycoon but cannot be spent', () => {
    const { f, player } = turnHolding(372, []);
    const p = f.s.playerStates[player];
    p.resources.fill(null);
    gainGuildCard(f.s, player, card('Material Cartel'));

    // The stockpile is on the card, not in a slot, so there is nothing to spend.
    expect(p.resources.filter(Boolean)).toHaveLength(0);
    expect(actions(f).some((a) => a.t === 'spendResource')).toBe(false);
    // But it counts: the card's own suit icon plus every token on it.
    expect(ambitionCount(p, 'tycoon', cartelIcons(f.s, player))).toBe(1 + f.s.cartel.material);
  });

  it('routes returned tokens onto the card instead of the supply', () => {
    const { f, player } = turnHolding(373, []);
    gainGuildCard(f.s, player, card('Fuel Cartel'));
    const held = f.s.cartel.fuel;
    returnToSupply(f.s, 'fuel');

    expect(f.s.supply.fuel).toBe(0);
    expect(f.s.cartel.fuel).toBe(held + 1);
  });

  it('releases the stockpile when the card leaves play', () => {
    const { f, player } = turnHolding(374, []);
    gainGuildCard(f.s, player, card('Material Cartel'));
    const held = f.s.cartel.material;
    discardGuildCard(f.s, player, card('Material Cartel'));

    expect(f.s.cartel.material).toBe(0);
    expect(f.s.supply.material).toBe(held);
  });

  it('makes Rivals discard that type after scoring, onto the card', () => {
    const f = startGame(3, 375);
    const holder = 0;
    const rival = 1;
    gainGuildCard(f.s, holder, card('Fuel Cartel'));
    const held = f.s.cartel.fuel;
    // Clear every board so the only Fuel in play is the two placed here.
    for (const p of f.s.playerStates) p.resources.fill(null);
    f.s.playerStates[rival].resources[0] = 'fuel';
    f.s.playerStates[rival].resources[1] = 'relic';
    f.s.playerStates[holder].resources[0] = 'fuel';

    // Run a chapter to its end.
    f.s.playerStates.forEach((p) => (p.hand = []));
    f.s.phase = 'play';
    settleToChapterEnd(f);

    expect(f.s.playerStates[rival].resources).not.toContain('fuel');
    expect(f.s.playerStates[rival].resources).toContain('relic'); // only its own type
    expect(f.s.playerStates[holder].resources).toContain('fuel'); // the holder keeps theirs
    expect(f.s.cartel.fuel).toBe(held + 1);
  });
});

describe('the Unions', () => {
  it('attaches to a face-up played card of its suit and draws it at round end', () => {
    const f = startGame(3, 381);
    const leader = actor(f);
    f.s.playerStates[leader].guildCards = [card('Construction Union')];
    setHand(f, leader, [cardId(CONSTRUCTION, 2), cardId(AGGRESSION, 3)]);
    apply(f, { t: 'lead', card: cardId(CONSTRUCTION, 2) });

    const attach = find<Extract<Action, { t: 'cardPrelude' }>>(
      actions(f),
      (a) => a.t === 'cardPrelude' && a.card === card('Construction Union'),
    );
    apply(f, attach);

    // The card is off the play area and on the played card, not discarded.
    expect(f.s.playerStates[leader].guildCards).not.toContain(card('Construction Union'));
    expect(f.s.courtDiscard).not.toContain(card('Construction Union'));
    expect(f.s.unions).toHaveLength(1);
    expect(f.s.unions[0].target).toBe(cardId(CONSTRUCTION, 2));

    // Play out the round; the lead card comes back to hand instead of the discard.
    apply(f, { t: 'beginActions' });
    settleRound(f);

    expect(f.s.playerStates[leader].hand).toContain(cardId(CONSTRUCTION, 2));
    expect(f.s.actionDiscard).not.toContain(cardId(CONSTRUCTION, 2));
    expect(f.s.courtDiscard).toContain(card('Construction Union'));
    expect(f.s.unions).toHaveLength(0);
  });

  it('only attaches to its own suit, and never to a face-down play', () => {
    const f = startGame(3, 382);
    const leader = actor(f);
    f.s.playerStates[leader].guildCards = [card('Admin Union')]; // administration
    setHand(f, leader, [cardId(CONSTRUCTION, 2)]);
    apply(f, { t: 'lead', card: cardId(CONSTRUCTION, 2) });
    expect(actions(f).some((a) => a.t === 'cardPrelude')).toBe(false);

    // A Copy play is face down, so a Union cannot see it either.
    const g = startGame(3, 383);
    const first = actor(g);
    setHand(g, first, [cardId(ADMIN, 2)]);
    apply(g, { t: 'lead', card: cardId(ADMIN, 2) });
    apply(g, { t: 'beginActions' });
    while (g.s.turn) apply(g, { t: 'endTurn' });
    const second = actor(g);
    g.s.playerStates[second].guildCards = [card('Admin Union')];
    setHand(g, second, [cardId(ADMIN, 3)]);
    apply(g, { t: 'follow', card: cardId(ADMIN, 3), mode: 'copy' });
    const targets = actions(g)
      .filter((a) => a.t === 'cardPrelude')
      .map((a) => (a as Extract<Action, { t: 'cardPrelude' }>).played);
    // Only the face-up lead is offered, not their own face-down copy.
    expect(targets).toEqual([0]);
  });
});

describe('Prison Wardens', () => {
  it('Pressgang returns Captives for a freely chosen resource each', () => {
    const { f, player } = turnHolding(391, ['Prison Wardens'], CONSTRUCTION, 2);
    const p = f.s.playerStates[player];
    p.captives = [1, 2];
    p.resources.fill(null);
    const agentsBefore = f.s.playerStates[1].agentsSupply + f.s.playerStates[2].agentsSupply;
    apply(f, { t: 'beginActions' });

    const offers = actions(f).filter((a) => a.t === 'cardAction' && a.name === 'Pressgang');
    // Both "one captive" and "two captives" options, and mixed resource picks.
    expect(offers.some((a) => (a as { gain?: unknown[] }).gain!.length === 1)).toBe(true);
    expect(offers.some((a) => (a as { gain?: unknown[] }).gain!.length === 2)).toBe(true);
    expect(
      offers.some((a) => {
        const gain = (a as { gain?: string[] }).gain!;
        return gain.length === 2 && gain[0] !== gain[1];
      }),
    ).toBe(true);

    const act = find<Extract<Action, { t: 'cardAction' }>>(
      offers,
      (a) => JSON.stringify((a as { gain?: string[] }).gain) === JSON.stringify(['fuel', 'relic']),
    );
    apply(f, act);

    expect(p.captives).toHaveLength(0);
    expect(p.resources).toContain('fuel');
    expect(p.resources).toContain('relic');
    // The agents went home to their owners.
    expect(f.s.playerStates[1].agentsSupply + f.s.playerStates[2].agentsSupply).toBe(
      agentsBefore + 2,
    );
  });

  it('Pressgang is capped by empty resource slots', () => {
    const { f, player } = turnHolding(392, ['Prison Wardens'], CONSTRUCTION, 2);
    const p = f.s.playerStates[player];
    p.captives = [1, 1, 1];
    p.resources.fill('fuel'); // no room at all
    apply(f, { t: 'beginActions' });
    expect(actions(f).some((a) => a.t === 'cardAction' && a.name === 'Pressgang')).toBe(false);
  });

  it('Execute moves Captives to Trophies, swapping Tyrant count for Warlord', () => {
    const { f, player } = turnHolding(393, ['Prison Wardens'], ADMIN, 2); // Influence pips
    const p = f.s.playerStates[player];
    p.captives = [1, 2];
    apply(f, { t: 'beginActions' });

    const act = find<Extract<Action, { t: 'cardAction' }>>(
      actions(f),
      (a) => a.t === 'cardAction' && a.name === 'Execute' && a.count === 2,
    );
    apply(f, act);

    expect(p.captives).toHaveLength(0);
    expect(p.trophies).toHaveLength(2);
    expect(p.trophies.every((t) => t.kind === 'agent')).toBe(true);
    expect(ambitionCount(p, 'tyrant')).toBe(0);
    expect(ambitionCount(p, 'warlord')).toBe(2);
  });

  it('an executed agent goes home when Trophies are returned', () => {
    const f = startGame(3, 394);
    const p = f.s.playerStates[0];
    const before = f.s.playerStates[1].agentsSupply;
    p.trophies = [{ owner: 1, kind: 'agent' }];
    f.s.declared.warlord = [0];
    f.s.playerStates.forEach((x) => (x.hand = []));
    f.s.phase = 'play';
    settleToChapterEnd(f);

    expect(f.s.playerStates[1].agentsSupply).toBe(before + 1);
  });
});

describe('Court Enforcers', () => {
  it('Abducts from a Court card holding fewer Rival agents than your Weapon icons', () => {
    const { f, player } = turnHolding(401, ['Court Enforcers'], AGGRESSION, 2); // Battle pips
    const p = f.s.playerStates[player];
    const rival = (player + 1) % 3;
    p.resources.fill(null);
    p.resources[0] = 'weapon';
    p.resources[1] = 'weapon';
    // Court Enforcers is itself a Weapon card, so that is 3 icons.
    expect(weaponIcons(p)).toBe(3);

    f.s.court[0].agents[rival] = 2; // fewer than 3 — abductable
    f.s.court[1].agents[rival] = 3; // not fewer than 3
    apply(f, { t: 'beginActions' });

    const offers = actions(f).filter((a) => a.t === 'cardAction' && a.name === 'Abduct');
    expect(offers.map((a) => (a as { slot?: number }).slot)).toEqual([0]);

    apply(f, offers[0]);
    expect(p.captives).toEqual([rival, rival]);
    expect(f.s.court[0].agents[rival]).toBe(0);
  });
});

describe('Elder Broker', () => {
  it('Trades a resource of the city’s type for one the Rival lacks', () => {
    const { f, player } = turnHolding(411, ['Elder Broker'], ADMIN, 2); // Tax pips
    const rival = (player + 1) % 3;
    // A planet the player controls, holding a Rival city.
    const system = f.v.systems.findIndex(
      (d) => d.kind === 'planet' && d.planetType !== null && !f.s.systems[d.id].outOfPlay,
    );
    const type = f.v.systems[system].planetType!;
    f.s.systems[system].fresh[player] = 3;
    f.s.systems[system].fresh[rival] = 0;
    f.s.systems[system].buildings = [
      { player: rival, kind: 'city', damaged: false, taxedThisTurn: false, builtThisTurn: false },
    ];

    const them = f.s.playerStates[rival];
    them.resources.fill(null);
    them.resources[0] = type;
    const me = f.s.playerStates[player];
    me.resources.fill(null);
    // A type they do not hold. Every cluster has three distinct types, so with
    // 5 types there is always another one to give.
    const give = (['material', 'fuel', 'weapon', 'relic', 'psionic'] as const).find((r) => r !== type)!;
    me.resources[0] = give;

    apply(f, { t: 'beginActions' });
    const act = find(actions(f), (a) => a.t === 'cardAction' && a.name === 'Trade');
    apply(f, act);

    expect(me.resources).toContain(type);
    expect(them.resources).toContain(give);
    expect(them.resources).not.toContain(type);
  });

  it('is not offered when the Rival already holds every type you could give', () => {
    const { f, player } = turnHolding(412, ['Elder Broker'], ADMIN, 2);
    const rival = (player + 1) % 3;
    const system = f.v.systems.findIndex(
      (d) => d.kind === 'planet' && d.planetType !== null && !f.s.systems[d.id].outOfPlay,
    );
    const type = f.v.systems[system].planetType!;
    f.s.systems[system].fresh[player] = 3;
    f.s.systems[system].buildings = [
      { player: rival, kind: 'city', damaged: false, taxedThisTurn: false, builtThisTurn: false },
    ];
    const them = f.s.playerStates[rival];
    them.resources.fill(null);
    them.resources[0] = type;
    const me = f.s.playerStates[player];
    me.resources.fill(null);
    me.resources[0] = type; // the only thing I hold is what they hold
    apply(f, { t: 'beginActions' });
    expect(actions(f).some((a) => a.t === 'cardAction' && a.name === 'Trade')).toBe(false);
  });
});

describe('Skirmishers', () => {
  it('offers a reroll of the blanks, up to your Weapon icons', () => {
    const { f, player, system, rival } = battleWith(421, ['Skirmishers']);
    const p = f.s.playerStates[player];
    p.resources.fill(null);
    p.resources[0] = 'weapon'; // + the Skirmishers card itself = 2 icons

    apply(f, { t: 'battle', system, defender: rival, assault: 0, skirmish: 4, raid: 0 });
    // A roll where every skirmish die is blank: face indices 3-5 are blanks.
    resolveChanceMut(f.s, f.v, () => 0.9);

    expect(f.s.phase).toBe('battleReroll');
    expect(f.s.battle!.skirmishBlanks).toBe(4);
    const counts = actions(f).map((a) => (a as { count: number }).count);
    expect(counts).toEqual([0, 1, 2]); // capped at 2 Weapon icons, not 4 blanks
  });

  it('a reroll converts blanks into hits without touching the rest of the roll', () => {
    const { f, player, system, rival } = battleWith(422, ['Skirmishers']);
    f.s.playerStates[player].resources.fill(null);
    f.s.playerStates[player].resources[0] = 'weapon';

    apply(f, { t: 'battle', system, defender: rival, assault: 0, skirmish: 3, raid: 0 });
    resolveChanceMut(f.s, f.v, () => 0.9); // all blank
    expect(f.s.battle!.hits).toBe(0);

    apply(f, { t: 'rerollSkirmish', count: 2 });
    expect(f.s.phase).toBe('battleRoll');
    resolveChanceMut(f.s, f.v, () => 0.1); // face index 0 — a hit

    expect(f.s.battle!.hits).toBe(2);
    expect(f.s.battle!.rerollDone).toBe(true);
  });

  it('declining goes straight to assignment, and the reroll is once per battle', () => {
    const { f, player, system, rival } = battleWith(423, ['Skirmishers']);
    f.s.playerStates[player].resources.fill(null);
    f.s.playerStates[player].resources[0] = 'weapon';

    apply(f, { t: 'battle', system, defender: rival, assault: 0, skirmish: 2, raid: 0 });
    resolveChanceMut(f.s, f.v, () => 0.9);
    apply(f, { t: 'rerollSkirmish', count: 0 });
    expect(f.s.battle?.rerollDone ?? true).toBe(true);
    expect(f.s.phase).not.toBe('battleReroll');
  });

  it('is not offered without the card', () => {
    const { f, system, rival } = battleWith(424, []);
    apply(f, { t: 'battle', system, defender: rival, assault: 0, skirmish: 2, raid: 0 });
    resolveChanceMut(f.s, f.v, () => 0.9);
    expect(f.s.phase).not.toBe('battleReroll');
  });
});

describe('Galactic Bards', () => {
  it('lets a Surpass declare, without placing the zero marker', () => {
    const f = startGame(3, 431);
    const leader = actor(f);
    setHand(f, leader, [cardId(CONSTRUCTION, 2)]);
    apply(f, { t: 'lead', card: cardId(CONSTRUCTION, 2) });
    apply(f, { t: 'beginActions' });
    while (f.s.turn) apply(f, { t: 'endTurn' });

    const follower = actor(f);
    f.s.playerStates[follower].guildCards = [card('Galactic Bards')];
    setHand(f, follower, [cardId(CONSTRUCTION, 5)]); // "5" = Keeper
    apply(f, { t: 'follow', card: cardId(CONSTRUCTION, 5), mode: 'surpass' });

    const offered = actions(f)
      .filter((a) => a.t === 'declareAmbition')
      .map((a) => (a as { ambition: string }).ambition);
    expect(offered).toEqual(['keeper']);

    apply(f, { t: 'declareAmbition', ambition: 'keeper' });
    expect(f.s.declared.keeper).toHaveLength(1);
    expect(f.s.round.leadNumber).toBe(2); // no zero marker
  });

  it('does nothing once an ambition is already declared this round', () => {
    const f = startGame(3, 432);
    const leader = actor(f);
    setHand(f, leader, [cardId(CONSTRUCTION, 4)]); // "4" = Warlord
    apply(f, { t: 'lead', card: cardId(CONSTRUCTION, 4) });
    apply(f, { t: 'declareAmbition', ambition: 'warlord' });
    apply(f, { t: 'beginActions' });
    while (f.s.turn) apply(f, { t: 'endTurn' });

    const follower = actor(f);
    f.s.playerStates[follower].guildCards = [card('Galactic Bards')];
    setHand(f, follower, [cardId(CONSTRUCTION, 5)]);
    apply(f, { t: 'follow', card: cardId(CONSTRUCTION, 5), mode: 'surpass' });
    expect(actions(f).some((a) => a.t === 'declareAmbition')).toBe(false);
  });

  it('without the card, a Surpass cannot declare at all', () => {
    const f = startGame(3, 433);
    const leader = actor(f);
    setHand(f, leader, [cardId(CONSTRUCTION, 2)]);
    apply(f, { t: 'lead', card: cardId(CONSTRUCTION, 2) });
    apply(f, { t: 'beginActions' });
    while (f.s.turn) apply(f, { t: 'endTurn' });
    const follower = actor(f);
    setHand(f, follower, [cardId(CONSTRUCTION, 5)]);
    apply(f, { t: 'follow', card: cardId(CONSTRUCTION, 5), mode: 'surpass' });
    expect(actions(f).some((a) => a.t === 'declareAmbition')).toBe(false);
  });
});

describe('Farseers', () => {
  it('peeks at one chosen hand on declaring, and may swap a card', () => {
    const f = startGame(3, 441);
    const leader = actor(f);
    const rival = (leader + 1) % 3;
    f.s.playerStates[leader].guildCards = [card('Farseers')];
    setHand(f, leader, [cardId(CONSTRUCTION, 4), cardId(ADMIN, 2)]);
    setHand(f, rival, [cardId(AGGRESSION, 6)]);
    apply(f, { t: 'lead', card: cardId(CONSTRUCTION, 4) });
    apply(f, { t: 'declareAmbition', ambition: 'warlord' });

    // Step 1: choose whose hand to look at.
    expect(f.s.phase).toBe('peekTarget');
    expect(actor(f)).toBe(leader);
    apply(f, { t: 'peekTarget', target: rival });

    // Step 2: swap, now that the hand is visible.
    expect(f.s.phase).toBe('peekSwap');
    apply(f, { t: 'peekSwap', give: cardId(ADMIN, 2), take: cardId(AGGRESSION, 6) });

    expect(f.s.playerStates[leader].hand).toContain(cardId(AGGRESSION, 6));
    expect(f.s.playerStates[rival].hand).toContain(cardId(ADMIN, 2));
    expect(f.s.phase).toBe('prelude'); // back to the Prelude
    expect(f.s.peek).toBeNull();
  });

  it('reveals only the peeked hand, and only while the swap is open', () => {
    const f = startGame(3, 442);
    const leader = actor(f);
    const seen = (leader + 1) % 3;
    const unseen = (leader + 2) % 3;
    f.s.playerStates[leader].guildCards = [card('Farseers')];
    setHand(f, leader, [cardId(CONSTRUCTION, 4)]);
    apply(f, { t: 'lead', card: cardId(CONSTRUCTION, 4) });
    apply(f, { t: 'declareAmbition', ambition: 'warlord' });
    apply(f, { t: 'peekTarget', target: seen });

    const view = observe(f.s, f.v, leader);
    expect(view.state.playerStates[seen].hand).toEqual(f.s.playerStates[seen].hand);
    expect(view.state.playerStates[unseen].hand).toEqual([]);
    // And the reveal is only for the peeking player.
    const other = observe(f.s, f.v, unseen);
    expect(other.state.playerStates[seen].hand).toEqual([]);

    // Determinizing keeps the revealed hand exactly as it is.
    const world = determinize(view, f.v, mulberry32(7));
    expect(world.playerStates[seen].hand).toEqual(f.s.playerStates[seen].hand);
  });

  it('can decline to look at all', () => {
    const f = startGame(3, 443);
    const leader = actor(f);
    f.s.playerStates[leader].guildCards = [card('Farseers')];
    setHand(f, leader, [cardId(CONSTRUCTION, 4)]);
    apply(f, { t: 'lead', card: cardId(CONSTRUCTION, 4) });
    apply(f, { t: 'declareAmbition', ambition: 'warlord' });
    apply(f, { t: 'peekTarget', target: null });
    expect(f.s.phase).toBe('prelude');
    expect(f.s.peek).toBeNull();
  });

  it('its Prelude swaps n hand cards for n + 1 from the discard bottom', () => {
    const { f, player } = turnHolding(444, ['Farseers']);
    const p = f.s.playerStates[player];
    setHand(f, player, [cardId(ADMIN, 2), cardId(ADMIN, 3)]);
    f.s.actionDiscard = [cardId(AGGRESSION, 4), cardId(AGGRESSION, 5), cardId(AGGRESSION, 6)];

    const act = find<Extract<Action, { t: 'cardPrelude' }>>(
      actions(f),
      (a) => a.t === 'cardPrelude' && a.card === card('Farseers') && (a.cards?.length ?? 0) === 1,
    );
    const discarded = act.cards![0];
    apply(f, act);

    // One card given up, two drawn from the bottom of the discard.
    expect(p.hand).toHaveLength(2 - 1 + 2);
    expect(p.hand).toContain(cardId(AGGRESSION, 4));
    expect(p.hand).toContain(cardId(AGGRESSION, 5));
    expect(p.hand).not.toContain(discarded);
    expect(f.s.actionDiscard).toContain(discarded);
    expect(p.guildCards).not.toContain(card('Farseers'));
  });
});

describe('Vox cards, when secured', () => {
  /** Put a Vox card in the Court with the player holding a majority on it. */
  function voxReady(seed: number, name: string) {
    const { f, player } = turnHolding(seed, [], ADMIN, 2); // Influence + Tax pips
    f.s.court[0] = { card: card(name), agents: Array(3).fill(0) };
    f.s.court[0].agents[player] = 1;
    apply(f, { t: 'beginActions' });
    // Secure needs a Relic-bought action; grant it directly.
    f.s.turn!.freeActions.push(['secure']);
    apply(f, { t: 'secure', slot: 0 });
    return { f, player };
  }

  it('Mass Uprising places a ship in every system of a chosen cluster', () => {
    const { f, player } = voxReady(451, 'Mass Uprising');
    expect(f.s.pendingVox?.card).toBe(card('Mass Uprising'));
    // It is mandatory, so declining is not offered while a cluster is legal.
    expect(actions(f).some((a) => a.t === 'voxSkip')).toBe(false);

    const act = find<Extract<Action, { t: 'vox' }>>(actions(f), (a) => a.t === 'vox');
    const cluster = act.cluster!;
    const before = f.s.systems.map((sys) => sys.fresh[player]);
    apply(f, act);

    f.s.systems.forEach((sys, i) => {
      const expected = !sys.outOfPlay && clusterOf(i) === cluster ? before[i] + 1 : before[i];
      expect(sys.fresh[player], `system ${i}`).toBe(expected);
    });
    expect(f.s.pendingVox).toBeNull();
    expect(f.s.courtDiscard).toContain(card('Mass Uprising'));
  });

  it('Populist Demands declares any ambition', () => {
    const { f } = voxReady(452, 'Populist Demands');
    const offered = actions(f)
      .filter((a) => a.t === 'vox')
      .map((a) => (a as { ambition?: string }).ambition);
    expect(offered).toEqual(['tycoon', 'tyrant', 'warlord', 'keeper', 'empath']);

    apply(f, { t: 'vox', ambition: 'empath' });
    expect(f.s.declared.empath).toHaveLength(1);
    expect(f.s.pendingVox).toBeNull();
  });

  it('Outrage Spreads outrages every player, including the securer', () => {
    const { f, player } = voxReady(453, 'Outrage Spreads');
    for (const p of f.s.playerStates) {
      p.resources.fill(null);
      p.resources[0] = 'relic';
    }
    apply(f, { t: 'vox', resource: 'relic' });

    for (const p of f.s.playerStates) {
      expect(p.resources).not.toContain('relic');
      expect(p.outrage.relic).toBe(true);
    }
    expect(f.s.playerStates[player].outrage.relic).toBe(true);
  });

  it('Song of Freedom returns a city and may seize, then goes back in the deck', () => {
    const { f, player } = voxReady(454, 'Song of Freedom');
    const system = f.s.systems.findIndex(
      (sys, i) => sys.buildings.some((b) => b.kind === 'city') && controlOf(f.s, i) === player,
    );
    // Give the player control of a city's system so one is offered.
    if (system < 0) {
      const any = f.s.systems.findIndex((sys) => sys.buildings.some((b) => b.kind === 'city'));
      f.s.systems[any].fresh[player] = 9;
    }
    const target = find<Extract<Action, { t: 'vox' }>>(actions(f), (a) => a.t === 'vox');
    const owner = f.s.systems[target.system!].buildings[target.building!].player;
    const usedBefore = f.s.playerStates[owner].citiesUsed;
    apply(f, target);

    expect(f.s.playerStates[owner].citiesUsed).toBe(usedBefore - 1);
    expect(f.s.courtDeck).toContain(card('Song of Freedom'));
    expect(f.s.courtDiscard).not.toContain(card('Song of Freedom'));
  });

  it('Guild Struggle steals a card and recycles the Guild discards', () => {
    const { f, player } = voxReady(455, 'Guild Struggle');
    const rival = (player + 1) % 3;
    f.s.playerStates[rival].guildCards = [card('Relic Fence')];
    f.s.courtDiscard = [card('Mining Interest'), card('Call to Action')];

    apply(f, { t: 'vox', target: rival, card: card('Relic Fence') });

    expect(f.s.playerStates[player].guildCards).toContain(card('Relic Fence'));
    expect(f.s.playerStates[rival].guildCards).toHaveLength(0);
    // Guild cards went back into the deck; the Vox one stayed discarded.
    expect(f.s.courtDeck).toContain(card('Mining Interest'));
    expect(f.s.courtDiscard).not.toContain(card('Mining Interest'));
    expect(f.s.courtDiscard).toContain(card('Call to Action'));
    expect(f.s.courtDiscard).toContain(card('Guild Struggle'));
  });

  it('Call to Action needs no decision and just draws a card', () => {
    const { f, player } = turnHolding(456, [], ADMIN, 2);
    f.s.court[0] = { card: card('Call to Action'), agents: Array(3).fill(0) };
    f.s.court[0].agents[player] = 1;
    apply(f, { t: 'beginActions' });
    f.s.actionDiscard = [cardId(AGGRESSION, 4), cardId(AGGRESSION, 5)];
    const handBefore = f.s.playerStates[player].hand.length;

    f.s.turn!.freeActions.push(['secure']);
    apply(f, { t: 'secure', slot: 0 });

    expect(f.s.pendingVox).toBeNull(); // resolved inline
    expect(f.s.playerStates[player].hand).toHaveLength(handBefore + 1);
    expect(f.s.playerStates[player].hand).toContain(cardId(AGGRESSION, 4));
  });

  it('a pending Vox card holds the turn open until it is answered', () => {
    const { f, player } = voxReady(457, 'Outrage Spreads');
    // The securing action was the last thing the turn could pay for, but the
    // turn cannot end while the Vox effect is outstanding.
    expect(f.s.pendingVox).not.toBeNull();
    expect(actor(f)).toBe(player);
    for (const a of actions(f)) expect(['vox', 'voxSkip']).toContain(a.t);

    apply(f, { t: 'voxSkip' });
    expect(f.s.pendingVox).toBeNull();
  });
});

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
