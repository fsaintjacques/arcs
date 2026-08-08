/** Component data: the map graph, the action deck, the dice, the markers. */
import { describe, expect, it } from 'vitest';
import {
  ALL_ACTION_CARDS,
  AMBITION_MARKERS,
  ASSAULT_FACES,
  buildSystems,
  CLUSTER_COUNT,
  COURT_DECK,
  DIE_FACES,
  PIPS_BY_NUMBER,
  RAID_FACES,
  SKIRMISH_FACES,
  SUIT_ACTIONS,
  actionDeckFor,
  clusterOf,
  gateId,
  isGate,
  makeVariant,
  planetId,
  resolveAdjacency,
} from '../src/engine';

describe('map', () => {
  const systems = buildSystems();

  it('has 6 clusters of 1 gate and 3 planets', () => {
    expect(systems).toHaveLength(24);
    for (let c = 0; c < CLUSTER_COUNT; c++) {
      const inCluster = systems.filter((s) => s.cluster === c);
      expect(inCluster).toHaveLength(4);
      expect(inCluster.filter((s) => s.kind === 'gate')).toHaveLength(1);
      expect(inCluster.filter((s) => s.kind === 'planet')).toHaveLength(3);
    }
  });

  it('gives every gate its 3 planets and its 2 neighbouring gates', () => {
    for (let c = 0; c < CLUSTER_COUNT; c++) {
      const gate = systems[gateId(c)];
      expect(gate.adjacent).toHaveLength(5);
      for (let p = 0; p < 3; p++) expect(gate.adjacent).toContain(planetId(c, p));
      const gateNeighbours = gate.adjacent.filter(isGate);
      expect(gateNeighbours).toHaveLength(2);
    }
  });

  it('makes planets adjacent to their gate and to one or both neighbours', () => {
    for (let c = 0; c < CLUSTER_COUNT; c++) {
      for (let p = 0; p < 3; p++) {
        const planet = systems[planetId(c, p)];
        expect(planet.adjacent).toContain(gateId(c));
        const planetNeighbours = planet.adjacent.filter((n) => !isGate(n));
        expect(planetNeighbours.length).toBeGreaterThanOrEqual(1);
        expect(planetNeighbours.length).toBeLessThanOrEqual(2);
      }
      // The outer two planets are separated by a thick border.
      expect(systems[planetId(c, 0)].adjacent).not.toContain(planetId(c, 2));
    }
  });

  it('gives every planet a type and 1-2 building slots', () => {
    for (const s of systems.filter((x) => x.kind === 'planet')) {
      expect(s.planetType).not.toBeNull();
      expect(s.buildingSlots).toBeGreaterThanOrEqual(1);
      expect(s.buildingSlots).toBeLessThanOrEqual(2);
    }
    for (const s of systems.filter((x) => x.kind === 'gate')) {
      expect(s.planetType).toBeNull();
      expect(s.buildingSlots).toBe(0);
    }
  });

  it('adjacency is symmetric', () => {
    for (const s of systems) {
      for (const n of s.adjacent) expect(systems[n].adjacent).toContain(s.id);
    }
  });

  it('path markers join the gates either side of an out-of-play cluster', () => {
    const resolved = resolveAdjacency(systems, [1]);
    // Cluster 1 is gone entirely.
    for (const s of resolved.filter((x) => x.cluster === 1)) expect(s.adjacent).toEqual([]);
    // Its neighbours' gates now touch each other directly.
    expect(resolved[gateId(0)].adjacent).toContain(gateId(2));
    expect(resolved[gateId(2)].adjacent).toContain(gateId(0));
    // And nothing still points into the dead cluster.
    for (const s of resolved) {
      for (const n of s.adjacent) expect(clusterOf(n)).not.toBe(1);
    }
  });

  it('keeps adjacency symmetric across two out-of-play clusters', () => {
    const resolved = resolveAdjacency(systems, [0, 3]);
    for (const s of resolved) {
      for (const n of s.adjacent) expect(resolved[n].adjacent).toContain(s.id);
    }
  });
});

describe('action deck', () => {
  it('is 4 suits x 7 numbers', () => {
    expect(ALL_ACTION_CARDS).toHaveLength(28);
  });

  it('drops the 1s and 7s below 4 players (p4 step C/D)', () => {
    expect(actionDeckFor(2)).toHaveLength(20);
    expect(actionDeckFor(3)).toHaveLength(20);
    expect(actionDeckFor(4)).toHaveLength(28);
    for (const c of actionDeckFor(3)) {
      expect(c.number).toBeGreaterThanOrEqual(2);
      expect(c.number).toBeLessThanOrEqual(6);
    }
  });

  it('has pips inversely related to the card number (p3)', () => {
    expect(PIPS_BY_NUMBER).toEqual({ 1: 4, 2: 4, 3: 3, 4: 3, 5: 2, 6: 2, 7: 1 });
    for (const c of ALL_ACTION_CARDS) expect(c.pips).toBe(PIPS_BY_NUMBER[c.number]);
  });

  it('maps card numbers to ambitions (p9)', () => {
    const byNumber = (n: number) => ALL_ACTION_CARDS.find((c) => c.number === n)!.ambition;
    expect(byNumber(1)).toBeNull();
    expect(byNumber(2)).toBe('tycoon');
    expect(byNumber(3)).toBe('tyrant');
    expect(byNumber(4)).toBe('warlord');
    expect(byNumber(5)).toBe('keeper');
    expect(byNumber(6)).toBe('empath');
    expect(byNumber(7)).toBe('any');
  });

  it('maps suits to their action lists (p8)', () => {
    expect(SUIT_ACTIONS.administration).toEqual(['tax', 'repair', 'influence']);
    expect(SUIT_ACTIONS.aggression).toEqual(['battle', 'move', 'secure']);
    expect(SUIT_ACTIONS.construction).toEqual(['build', 'repair']);
    expect(SUIT_ACTIONS.mobilization).toEqual(['move', 'influence']);
  });
});

describe('court deck', () => {
  it('is 25 Guild plus 6 Vox (p4 step H)', () => {
    expect(COURT_DECK).toHaveLength(31);
    expect(COURT_DECK.filter((c) => c.kind === 'guild')).toHaveLength(25);
    expect(COURT_DECK.filter((c) => c.kind === 'vox')).toHaveLength(6);
  });

  it('gives every Guild card a resource suit and a raid cost (p17)', () => {
    for (const c of COURT_DECK.filter((x) => x.kind === 'guild')) {
      expect(c.suit).not.toBeNull();
      expect(c.raidCost).toBeGreaterThan(0);
    }
  });
});

describe('battle dice', () => {
  it('has 6 faces per die', () => {
    for (const faces of Object.values(DIE_FACES)) expect(faces).toHaveLength(6);
  });

  // Faces are printed in full in the official aid booklet (p3), so these are
  // exact tables rather than distributions inferred from quoted odds.
  const tally = (faces: typeof SKIRMISH_FACES) =>
    faces
      .map((f) => `${f.hits}h${f.selfHits}s${f.buildingHits}b${f.keys}k${f.intercept}i`)
      .sort();

  it('skirmish is 3 single hits and 3 blanks, and never hurts the attacker', () => {
    expect(tally(SKIRMISH_FACES)).toEqual(
      ['1h0s0b0k0i', '1h0s0b0k0i', '1h0s0b0k0i', '0h0s0b0k0i', '0h0s0b0k0i', '0h0s0b0k0i'].sort(),
    );
  });

  it('assault is 2 hits / 2 hits+self / hit+intercept / 2x hit+self / blank', () => {
    expect(tally(ASSAULT_FACES)).toEqual(
      ['2h0s0b0k0i', '2h1s0b0k0i', '1h0s0b0k1i', '1h1s0b0k0i', '1h1s0b0k0i', '0h0s0b0k0i'].sort(),
    );
  });

  it('raid rolls keys and building hits and never hits defending ships', () => {
    expect(tally(RAID_FACES)).toEqual(
      ['0h0s0b2k1i', '0h1s0b1k0i', '0h0s1b1k0i', '0h1s1b0k0i', '0h1s1b0k0i', '0h0s0b0k1i'].sort(),
    );
    expect(RAID_FACES.every((f) => f.hits === 0)).toBe(true);
  });

  it('matches the odds quoted for each die', () => {
    // Assault: >=1 hit on 5 of 6, 2 hits on 2 of 6, self-hit on 3, intercept on 1.
    expect(ASSAULT_FACES.filter((f) => f.hits >= 1)).toHaveLength(5);
    expect(ASSAULT_FACES.filter((f) => f.hits === 2)).toHaveLength(2);
    expect(ASSAULT_FACES.filter((f) => f.selfHits > 0)).toHaveLength(3);
    expect(ASSAULT_FACES.filter((f) => f.intercept > 0)).toHaveLength(1);
    // Raid: keys on half the faces, a building hit on half.
    expect(RAID_FACES.filter((f) => f.keys > 0)).toHaveLength(3);
    expect(RAID_FACES.filter((f) => f.buildingHits > 0)).toHaveLength(3);
    // Raid is the riskiest: it intercepts on two faces where assault does on one.
    expect(RAID_FACES.filter((f) => f.intercept > 0).length).toBeGreaterThan(
      ASSAULT_FACES.filter((f) => f.intercept > 0).length,
    );
  });
});

describe('ambition markers', () => {
  it('starts on the blue sides shown in the rulebook (p3)', () => {
    expect(AMBITION_MARKERS.map((m) => m.blue)).toEqual([
      { first: 5, second: 3 },
      { first: 3, second: 2 },
      { first: 2, second: 0 },
    ]);
  });

  it('is worth more on the orange side', () => {
    for (const m of AMBITION_MARKERS) {
      expect(m.orange.first).toBeGreaterThan(m.blue.first);
      expect(m.orange.second).toBeGreaterThanOrEqual(m.blue.second);
    }
  });
});

describe('variant', () => {
  it('sets the Power threshold and Court row by player count (p2, p4)', () => {
    expect(makeVariant(2).powerThreshold).toBe(33);
    expect(makeVariant(3).powerThreshold).toBe(30);
    expect(makeVariant(4).powerThreshold).toBe(27);
    expect(makeVariant(2).courtRowSize).toBe(3);
    expect(makeVariant(3).courtRowSize).toBe(4);
    expect(makeVariant(4).courtRowSize).toBe(4);
  });

  it('removes 2 clusters below 4 players and 1 at 4 players (p4 step J)', () => {
    for (const players of [2, 3]) {
      const v = makeVariant(players);
      const dead = new Set(v.systems.filter((s) => s.adjacent.length === 0).map((s) => s.cluster));
      expect(dead.size).toBe(2);
    }
    const v4 = makeVariant(4);
    const dead4 = new Set(v4.systems.filter((s) => s.adjacent.length === 0).map((s) => s.cluster));
    expect(dead4.size).toBe(1);
  });
});
