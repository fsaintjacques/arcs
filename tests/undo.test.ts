/**
 * The undo reveal-barrier predicate: undo must never carry a player back
 * across newly revealed hidden information. The predicate reads state deltas
 * rather than action types, because reveals happen transitively (a destroyed
 * city Ransacks the Court, securing a Vox card resolves it mid-action).
 */
import { describe, expect, it } from 'vitest';
import { applyActionMut } from '../src/engine';
import { markInfo, revealedSince } from '../src/ui/useGame';
import { actions, AGGRESSION, cardId, find, setHand, startGame } from './helpers';

describe('undo reveal barrier', () => {
  it('ignores actions that reveal nothing', () => {
    const f = startGame(3, 1);
    const humans = [0];
    const before = markInfo(f.s, humans);
    expect(revealedSince(f.s, humans, before)).toBe(false);
  });

  it('fires when the Court deck shrinks — a hidden card flipped up', () => {
    const f = startGame(3, 1);
    const humans = [0];
    const before = markInfo(f.s, humans);
    f.s.court[0].card = f.s.courtDeck.pop()!;
    expect(revealedSince(f.s, humans, before)).toBe(true);
  });

  it('fires when a human hand gains a card, but not when it loses one', () => {
    const f = startGame(3, 1);
    const humans = [0];
    const hand = f.s.playerStates[0].hand;

    const beforeLoss = markInfo(f.s, humans);
    const played = hand.pop()!;
    expect(revealedSince(f.s, humans, beforeLoss), 'playing a card is not a reveal').toBe(false);

    const beforeGain = markInfo(f.s, humans);
    hand.push(played === 0 ? 1 : 0);
    expect(revealedSince(f.s, humans, beforeGain), 'drawing one is').toBe(true);
  });

  it('does not fire for a rival hand changing — bots learn nothing durable', () => {
    const f = startGame(3, 1);
    const humans = [0];
    const before = markInfo(f.s, humans);
    f.s.playerStates[1].hand.push(27);
    expect(revealedSince(f.s, humans, before)).toBe(false);
  });

  it('fires when a Farseers peek opens a hand', () => {
    const f = startGame(3, 1);
    const humans = [0];
    const before = markInfo(f.s, humans);
    f.s.peek = { player: 0, target: 1, returnPhase: 'actions' } as never;
    expect(revealedSince(f.s, humans, before)).toBe(true);
  });

  it('fires through a real secure, which refills the Court row', () => {
    const f = startGame(3, 2);
    const humans = [0];
    // The leader gets an Aggression card (Secure is an Aggression action) and
    // a strict majority of agents on Court slot 0.
    const player = f.s.round.turnOrder[0];
    f.s.court[0].agents[player] = 2;
    setHand(f, player, [cardId(AGGRESSION, 4)]);

    const lead = find(actions(f), (a) => a.t === 'lead');
    applyActionMut(f.s, f.v, lead);
    const begin = actions(f).find((a) => a.t === 'beginActions');
    if (begin) applyActionMut(f.s, f.v, begin);

    const secure = find(actions(f), (a) => a.t === 'secure');
    const before = markInfo(f.s, humans);
    applyActionMut(f.s, f.v, secure);
    expect(revealedSince(f.s, humans, before)).toBe(true);
  });
});
