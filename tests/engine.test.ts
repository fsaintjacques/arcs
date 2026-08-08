/**
 * The decision-process contract: determinism, purity, termination, and the
 * imperfect-information boundary that agents rely on.
 */
import { describe, expect, it } from 'vitest';
import {
  applyAction,
  applyActionMut,
  cloneState,
  COURT_DECK,
  determinize,
  getPending,
  makeVariant,
  mulberry32,
  newGame,
  observe,
  resolveChanceMut,
  type GameState,
  type VariantDef,
} from '../src/engine';
import { makeAgent } from '../src/agents';
import { playGame, permutations } from '../src/sim/runner';

/** Drive a game to the end with a policy, returning every state it passed. */
function driveTo(v: VariantDef, s: GameState, rng: () => number, pick: (n: number) => number) {
  let decisions = 0;
  for (let guard = 0; guard < 100_000; guard++) {
    const node = getPending(s, v);
    if (node.kind === 'over') break;
    if (node.kind === 'chance') {
      resolveChanceMut(s, v, rng);
      continue;
    }
    expect(node.actions.length).toBeGreaterThan(0);
    applyActionMut(s, v, node.actions[pick(node.actions.length)]);
    decisions++;
  }
  return decisions;
}

describe('decision process', () => {
  it('always offers at least one action at a decision node, and terminates', () => {
    for (const players of [2, 3, 4]) {
      for (let seed = 0; seed < 12; seed++) {
        const v = makeVariant(players, seed);
        const rng = mulberry32(seed + 1);
        const s = newGame(v, rng, seed);
        const decisions = driveTo(v, s, rng, (n) => Math.floor(rng() * n));
        expect(s.phase).toBe('over');
        expect(decisions).toBeGreaterThan(20);
      }
    }
  });

  it('terminates under a "never pass, never idle" policy too', () => {
    // Adversarial for termination: this policy prefers actions that keep the
    // turn alive, which is exactly what a catapult loop would exploit.
    for (const players of [2, 3, 4]) {
      for (let seed = 0; seed < 8; seed++) {
        const v = makeVariant(players, seed);
        const rng = mulberry32(seed + 100);
        const s = newGame(v, rng, seed);
        for (let guard = 0; guard < 100_000; guard++) {
          const node = getPending(s, v);
          if (node.kind === 'over') break;
          if (node.kind === 'chance') {
            resolveChanceMut(s, v, rng);
            continue;
          }
          const busy = node.actions.filter(
            (a) => a.t !== 'endTurn' && a.t !== 'passInitiative' && a.t !== 'catapultStop',
          );
          const pool = busy.length > 0 ? busy : node.actions;
          applyActionMut(s, v, pool[Math.floor(rng() * pool.length)]);
        }
        expect(s.phase).toBe('over');
      }
    }
  });

  it('survives a game where everyone holds the entire Court', () => {
    // Every Guild and Vox ability is reachable but most are rare in ordinary
    // play — a specific card reaches a hand maybe once per few games. Dealing
    // the whole Court to everyone forces every Prelude, new action and passive
    // to be enumerated and applied thousands of times, which is what catches a
    // dispatch that throws rather than one that is merely never chosen.
    const everyCard = COURT_DECK.filter((c) => c.kind === 'guild').map((c) => c.id);
    const seen = new Set<string>();

    for (const players of [2, 3, 4]) {
      for (let seed = 0; seed < 4; seed++) {
        const v = makeVariant(players, seed);
        const rng = mulberry32(seed + 900);
        const s = newGame(v, rng, seed);
        for (const p of s.playerStates) p.guildCards = [...everyCard];
        // Both Cartels are now held by everyone, so those supplies are empty.
        for (const type of ['material', 'fuel'] as const) {
          s.cartel[type] += s.supply[type];
          s.supply[type] = 0;
        }
        // Captives and Weapon icons, so Pressgang / Execute / Abduct qualify.
        for (let p = 0; p < players; p++) {
          s.playerStates[p].captives = [(p + 1) % players, (p + 1) % players];
          s.court.forEach((slot) => (slot.agents[(p + 1) % players] = 1));
        }

        for (let guard = 0; guard < 200_000; guard++) {
          const node = getPending(s, v);
          if (node.kind === 'over') break;
          if (node.kind === 'chance') {
            resolveChanceMut(s, v, rng);
            continue;
          }
          expect(node.actions.length).toBeGreaterThan(0);
          // Prefer an ability over an ordinary action, to drive them hard.
          const fancy = node.actions.filter(
            (a) => a.t === 'cardPrelude' || a.t === 'cardAction' || a.t === 'vox' || a.t === 'rerollSkirmish',
          );
          const pool = fancy.length > 0 && rng() < 0.8 ? fancy : node.actions;
          const chosen = pool[Math.floor(rng() * pool.length)];
          if (chosen.t === 'cardAction') seen.add(chosen.name);
          if (chosen.t === 'cardPrelude') {
            if (chosen.played !== undefined) seen.add('union');
            if (chosen.cards !== undefined) seen.add('farseers');
            if (chosen.takeCard !== undefined) seen.add('stealCard');
          }
          if (chosen.t === 'rerollSkirmish' && chosen.count > 0) seen.add('reroll');
          if (chosen.t === 'vox') seen.add('vox');
          applyActionMut(s, v, chosen);
        }
        expect(s.phase).toBe('over');
      }
    }

    // Confirm the hard-to-reach paths really did run, so this test cannot pass
    // by never exercising anything.
    //
    // Elder Broker's Trade is absent on purpose: it needs a Rival city sitting
    // in a system you control, that Rival holding the planet's resource, and you
    // holding a type they lack — a conjunction a random walk pulls apart faster
    // than it assembles. It is covered directly in powers.test.ts instead.
    for (const path of ['Pressgang', 'Execute', 'Abduct', 'union', 'farseers', 'stealCard', 'reroll', 'vox']) {
      expect(seen, path).toContain(path);
    }
    // Manufacture and Synthesize are also absent, and that is the Cartels
    // working: with both held, the Material and Fuel supplies are empty, so
    // neither "gain 1 of that resource" action can be offered at all.
    expect(seen).not.toContain('Manufacture');
    expect(seen).not.toContain('Synthesize');
  });

  it('applyAction is pure: the input state is untouched', () => {
    const v = makeVariant(3, 2);
    const rng = mulberry32(7);
    const s = newGame(v, rng, 2);
    resolveChanceMut(s, v, rng);
    const node = getPending(s, v);
    if (node.kind !== 'decision') throw new Error('expected a decision');

    const before = JSON.stringify(s);
    const next = applyAction(s, v, node.actions[0]);
    expect(JSON.stringify(s)).toBe(before);
    expect(JSON.stringify(next)).not.toBe(before);
  });

  it('cloneState deep-copies every mutable branch', () => {
    const v = makeVariant(4, 1);
    const rng = mulberry32(3);
    const s = newGame(v, rng, 1);
    resolveChanceMut(s, v, rng);
    const copy = cloneState(s);

    copy.systems[0].fresh[0] = 99;
    copy.playerStates[0].hand.push(0);
    copy.playerStates[0].resources[0] = 'relic';
    copy.playerStates[0].trophies.push({ owner: 1, kind: 'ship' });
    copy.court[0].agents[0] = 5;
    copy.declared.tycoon.push(0);
    copy.supply.fuel = 0;

    expect(s.systems[0].fresh[0]).not.toBe(99);
    expect(s.playerStates[0].hand).not.toContain(0);
    expect(s.playerStates[0].trophies).toHaveLength(0);
    expect(s.court[0].agents[0]).not.toBe(5);
    expect(s.declared.tycoon).toHaveLength(0);
    expect(s.supply.fuel).not.toBe(0);
  });

  it('is reproducible from a seed', () => {
    const agents = [makeAgent('greedy'), makeAgent('greedy'), makeAgent('random')];
    const a = playGame(agents, { players: 3, seed: 4242, setupIndex: 2 });
    const b = playGame(agents, { players: 3, seed: 4242, setupIndex: 2 });
    expect(b.power).toEqual(a.power);
    expect(b.winner).toBe(a.winner);
    expect(b.decisions).toBe(a.decisions);
  });

  it('conserves pieces: ships on the map plus supply plus Trophies equals 15', () => {
    const v = makeVariant(3, 0);
    const rng = mulberry32(11);
    const s = newGame(v, rng, 0);
    driveTo(v, s, rng, (n) => Math.floor(rng() * n));

    for (let p = 0; p < 3; p++) {
      const onMap = s.systems.reduce((n, sys) => n + sys.fresh[p] + sys.damaged[p], 0);
      const asTrophies = s.playerStates.reduce(
        (n, other) => n + other.trophies.filter((t) => t.owner === p && t.kind === 'ship').length,
        0,
      );
      expect(onMap + s.playerStates[p].shipsSupply + asTrophies).toBe(15);
    }
  });

  it('never leaves a piece in an out-of-play cluster', () => {
    const v = makeVariant(2, 3);
    const rng = mulberry32(12);
    const s = newGame(v, rng, 3);
    driveTo(v, s, rng, (n) => Math.floor(rng() * n));
    for (const sys of s.systems) {
      if (!sys.outOfPlay) continue;
      expect(sys.fresh.reduce((a, b) => a + b, 0)).toBe(0);
      expect(sys.damaged.reduce((a, b) => a + b, 0)).toBe(0);
      expect(sys.buildings).toHaveLength(0);
    }
  });
});

describe('imperfect information', () => {
  function midGame(players = 3, seed = 21) {
    const v = makeVariant(players, 1);
    const rng = mulberry32(seed);
    const s = newGame(v, rng, 1);
    resolveChanceMut(s, v, rng);
    // Play a handful of decisions so there is a trick in progress.
    for (let i = 0; i < 6; i++) {
      const node = getPending(s, v);
      if (node.kind === 'over') break;
      if (node.kind === 'chance') {
        resolveChanceMut(s, v, rng);
        continue;
      }
      applyActionMut(s, v, node.actions[Math.floor(rng() * node.actions.length)]);
    }
    return { v, s, rng };
  }

  it('hides Rival hands and every deck order', () => {
    const { v, s } = midGame();
    const obs = observe(s, v, 0);
    expect(obs.state.playerStates[0].hand).toEqual(s.playerStates[0].hand);
    for (let p = 1; p < s.players; p++) expect(obs.state.playerStates[p].hand).toEqual([]);
    expect(obs.state.actionDeck).toEqual([]);
    expect(obs.state.actionDiscard).toEqual([]);
    expect(obs.state.courtDeck).toEqual([]);
  });

  it('reports Rival hand sizes even though the cards are hidden', () => {
    const { v, s } = midGame();
    const obs = observe(s, v, 0);
    expect(obs.handSizes).toEqual(s.playerStates.map((p) => p.hand.length));
  });

  it('hides Rivals’ face-down plays but not the observer’s own', () => {
    const { v, s } = midGame(4, 33);
    s.round.played.push({ player: 1, card: 5, mode: 'copy', faceDown: true });
    s.round.played.push({ player: 0, card: 6, mode: 'copy', faceDown: true });
    const obs = observe(s, v, 0);
    const mine = obs.state.round.played.find((c) => c.player === 0)!;
    const theirs = obs.state.round.played.find((c) => c.player === 1)!;
    expect(mine.card).toBe(6);
    expect(theirs.card).toBe(-1);
  });

  it('determinize rebuilds a legal, playable world', () => {
    const { v, s } = midGame();
    const obs = observe(s, v, 0);
    for (let i = 0; i < 10; i++) {
      const world = determinize(obs, v, mulberry32(100 + i));
      expect(world.playerStates[0].hand).toEqual(s.playerStates[0].hand);
      for (let p = 0; p < world.players; p++) {
        expect(world.playerStates[p].hand).toHaveLength(obs.handSizes[p]);
      }
      // No card exists twice across hands.
      const all = world.playerStates.flatMap((p) => p.hand);
      expect(new Set(all).size).toBe(all.length);
      // And the sampled world can actually be played.
      const node = getPending(world, v);
      expect(node.kind === 'over' || node.kind === 'chance' || node.actions.length > 0).toBe(true);
    }
  });

  it('samples genuinely different worlds', () => {
    const { v, s } = midGame(4, 44);
    const obs = observe(s, v, 0);
    const seen = new Set<string>();
    for (let i = 0; i < 8; i++) {
      seen.add(JSON.stringify(determinize(obs, v, mulberry32(500 + i)).playerStates[1].hand));
    }
    expect(seen.size).toBeGreaterThan(1);
  });

  it('never invents a card the observer can already account for', () => {
    const { v, s } = midGame(3, 55);
    const obs = observe(s, v, 0);
    const mine = new Set(s.playerStates[0].hand);
    for (let i = 0; i < 10; i++) {
      const world = determinize(obs, v, mulberry32(900 + i));
      for (let p = 1; p < world.players; p++) {
        for (const card of world.playerStates[p].hand) expect(mine.has(card)).toBe(false);
      }
    }
  });
});

describe('seating', () => {
  it('enumerates every permutation', () => {
    expect(permutations(2)).toHaveLength(2);
    expect(permutations(3)).toHaveLength(6);
    expect(permutations(4)).toHaveLength(24);
    for (const p of permutations(4)) expect([...p].sort()).toEqual([0, 1, 2, 3]);
  });
});
