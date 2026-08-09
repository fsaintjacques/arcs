/** The evaluation orders positions the way the game's incentives do. */
import { describe, expect, it } from 'vitest';
import {
  applyAction,
  COURT_DECK,
  determinize,
  getPending,
  makeVariant,
  mulberry32,
  newGame,
  observe,
  resolveChanceMut,
} from '../src/engine';
import {
  anchorWeightsV0,
  defaultWeights,
  evaluate,
  makeGreedy,
  relativeEvaluate,
} from '../src/agents';
import { ADMIN, AGGRESSION, CONSTRUCTION, MOBILIZATION, actions, actor, apply, cardId, setHand, startGame } from './helpers';

function card(name: string): number {
  const def = COURT_DECK.find((c) => c.name === name);
  if (!def) throw new Error(`no court card named ${name}`);
  return def.id;
}

describe('hand quality', () => {
  it('a card in hand never lowers the evaluation', () => {
    const f = startGame(3, 21);
    const player = actor(f);
    setHand(f, player, []);
    const empty = evaluate(f.s, f.v, player);
    setHand(f, player, [cardId(ADMIN, 4)]);
    expect(evaluate(f.s, f.v, player)).toBeGreaterThan(empty);
  });

  it('prefers pips: a 2 outvalues a 6 of the same suit', () => {
    const f = startGame(3, 21);
    const player = actor(f);
    // Both cards outrank nothing (the 6 would be top card), so isolate pips
    // by giving a rival the 6-beating... instead compare with high-card term zeroed.
    const w = { ...defaultWeights, handHighCard: 0 };
    setHand(f, player, [cardId(CONSTRUCTION, 2)]);
    const lowNumber = evaluate(f.s, f.v, player, w);
    setHand(f, player, [cardId(CONSTRUCTION, 6)]);
    const highNumber = evaluate(f.s, f.v, player, w);
    expect(lowNumber).toBeGreaterThan(highNumber);
  });

  it('values holding the highest live card of a suit', () => {
    const f = startGame(3, 21);
    const player = actor(f);
    const rival = (player + 1) % 3;
    setHand(f, player, [cardId(AGGRESSION, 5)]);
    setHand(f, (player + 2) % 3, [cardId(ADMIN, 2)]);
    // While the rival holds the 6, my 5 can always be Surpassed.
    setHand(f, rival, [cardId(AGGRESSION, 6)]);
    const surpassable = evaluate(f.s, f.v, player);
    // Once the 6 is out of hands, the 5 is the boss card.
    setHand(f, rival, [cardId(MOBILIZATION, 3)]);
    const boss = evaluate(f.s, f.v, player);
    expect(boss).toBeGreaterThan(surpassable);
  });
});

describe('resource scarcity', () => {
  it('prices the scarce ambition resources above the common ones', () => {
    const f = startGame(3, 22);
    const player = actor(f);
    const p = f.s.playerStates[player];
    p.resources[0] = 'weapon';
    const weapon = evaluate(f.s, f.v, player);
    p.resources[0] = 'psionic';
    const psionic = evaluate(f.s, f.v, player);
    expect(psionic).toBeGreaterThan(weapon);
  });
});

describe('ambition timing', () => {
  it('values strictly leading an undeclared ambition while markers remain', () => {
    const f = startGame(3, 23);
    const player = actor(f);
    const p = f.s.playerStates[player];
    p.resources.fill(null);
    p.captives = [];
    const behind = evaluate(f.s, f.v, player);
    p.captives = [((player + 1) % 3), ((player + 1) % 3)]; // strict Tyrant lead
    const leading = evaluate(f.s, f.v, player);
    expect(leading).toBeGreaterThan(behind);
  });

  it('the anchor weights ignore the new terms entirely', () => {
    const f = startGame(3, 23);
    const player = actor(f);
    const before = evaluate(f.s, f.v, player, anchorWeightsV0);
    f.s.playerStates[player].captives = [(player + 1) % 3, (player + 1) % 3];
    const after = evaluate(f.s, f.v, player, anchorWeightsV0);
    // Captives still count through the latent term, but declarableLead adds 0.
    expect(after - before).toBeCloseTo(2 * anchorWeightsV0.latentAmbition, 10);
  });
});

describe('the hand-quality blind spot', () => {
  // FINDINGS.md: over 40 greedy games the 2p mulligan and Farseers' recycle
  // were decided by a count-only hand term, which cannot tell a hand of 2s
  // from a hand of 6s. Both decisions now turn on what the hand is worth.

  /** A 2-player game paused at the mulligan decision, hand replaced. */
  function atMulligan(seed: number, hand: number[]) {
    const v = makeVariant(2, 0);
    const rng = mulberry32(seed);
    const s = newGame(v, rng, 0);
    resolveChanceMut(s, v, rng); // deal
    if (s.phase !== 'mulligan') throw new Error('expected the mulligan phase');
    const player = (s.initiative + 1) % 2;
    for (const c of s.playerStates[player].hand) if (!hand.includes(c)) s.actionDiscard.push(c);
    s.playerStates[player].hand = [...hand];
    return { v, s, player };
  }

  /** How often greedy takes the mulligan across sampled worlds. */
  function mulliganRate(hand: number[]): number {
    let takes = 0;
    for (let seed = 1; seed <= 9; seed++) {
      const { v, s, player } = atMulligan(31, hand);
      const greedy = makeGreedy();
      const obs = observe(s, v, player);
      const node = getPending(s, v);
      if (node.kind !== 'decision') throw new Error('expected a decision');
      const chosen = greedy.choose(obs, node.actions, { variant: v, rng: mulberry32(seed), player });
      if (chosen.t === 'mulligan' && chosen.take) takes++;
    }
    return takes / 9;
  }

  it('greedy mulligans a junk hand and keeps a premium one', () => {
    // Four 5s and two 6s: 12 pips against an expected ~17 from a redraw.
    const junk = [
      cardId(ADMIN, 5), cardId(AGGRESSION, 5), cardId(CONSTRUCTION, 5),
      cardId(MOBILIZATION, 5), cardId(ADMIN, 6), cardId(AGGRESSION, 6),
    ];
    // Four 2s and two 3s: 22 pips, better than any redraw can promise.
    const premium = [
      cardId(ADMIN, 2), cardId(AGGRESSION, 2), cardId(CONSTRUCTION, 2),
      cardId(MOBILIZATION, 2), cardId(ADMIN, 3), cardId(AGGRESSION, 3),
    ];
    expect(mulliganRate(junk)).toBeGreaterThan(0.5);
    expect(mulliganRate(premium)).toBeLessThan(0.5);
  });

  it("Farseers' recycle now reads better the worse the hand is", () => {
    // The recycle stays expensive — it costs the Guild card, its psionic
    // icon, and possibly a rival's empath lead — so greedy declining it is
    // often correct pricing, not blindness. What the hand terms change is
    // that the price now depends on what would be thrown away.
    const value = (hand: number[]) => {
      const f = startGame(3, 47);
      const player = actor(f);
      f.s.playerStates[player].guildCards = [card('Farseers')];
      setHand(f, player, [...hand, cardId(MOBILIZATION, 6)]);
      apply(f, { t: 'lead', card: cardId(MOBILIZATION, 6) });

      const recycle = actions(f).find(
        (a) => a.t === 'cardPrelude' && (a.cards?.length ?? 0) === hand.length,
      );
      if (!recycle) throw new Error('recycle of the full hand not offered');
      const world = determinize(observe(f.s, f.v, player), f.v, mulberry32(7));
      const base = relativeEvaluate(world, f.v, player, defaultWeights);
      return relativeEvaluate(applyAction(world, f.v, recycle), f.v, player, defaultWeights) - base;
    };

    const junk = [cardId(ADMIN, 5), cardId(AGGRESSION, 5), cardId(CONSTRUCTION, 5)];
    const premium = [cardId(ADMIN, 2), cardId(AGGRESSION, 2), cardId(CONSTRUCTION, 2)];
    expect(value(junk)).toBeGreaterThan(value(premium));
  });
});
