/**
 * Face-down plays stay face down in the UI.
 *
 * A Copy and the card burned to seize the initiative go down face down and are
 * never turned back up, so neither the trick panel nor the log may name them —
 * the panels draw the true state rather than an observation, so the redaction
 * is theirs to do.
 */
import { describe, expect, it } from 'vitest';
import { renderToStaticMarkup } from 'react-dom/server';
import { actionCard, makeVariant, mulberry32, newGame } from '../src/engine';
import { Trick } from '../src/ui/components/Panels';
import { describeForLog } from '../src/ui/useGame';
import { cardLabel } from '../src/ui/describe';

const v = makeVariant(3, 0);
/** Two cards of different suits, so one showing cannot be mistaken for the other. */
const MINE = v.actionDeck.find((c) => c.suit === 'aggression')!.id;
const THEIRS = v.actionDeck.find((c) => c.suit === 'construction')!.id;

function trick(played: { player: number; card: number; mode: string; faceDown: boolean }[]) {
  const s = newGame(v, mulberry32(4), 0);
  s.round.played = played as typeof s.round.played;
  return renderToStaticMarkup(<Trick state={s} humanSeats={[0]} />);
}

describe('face-down plays in the trick panel', () => {
  it('hides a Rival Copy and shows your own', () => {
    const html = trick([
      { player: 0, card: MINE, mode: 'copy', faceDown: true },
      { player: 1, card: THEIRS, mode: 'copy', faceDown: true },
    ]);
    expect(html).toContain('card-back');
    expect(html).toContain(actionCard(MINE).suit);
    expect(html).not.toContain(actionCard(THEIRS).suit);
  });

  it('hides the card a Rival burned to seize, and labels it seized', () => {
    const html = trick([{ player: 1, card: THEIRS, mode: 'follow', faceDown: true }]);
    expect(html).toContain('card-back');
    expect(html).toContain('seized');
    expect(html).not.toContain(actionCard(THEIRS).suit);
  });

  it('leaves face-up plays alone', () => {
    const html = trick([{ player: 1, card: THEIRS, mode: 'surpass', faceDown: false }]);
    expect(html).not.toContain('card-back');
    expect(html).toContain(actionCard(THEIRS).suit);
  });
});

describe('face-down plays in the log', () => {
  const s = newGame(v, mulberry32(4), 0);

  it('does not name a Rival’s seized or copied card', () => {
    for (const a of [
      { t: 'seize', card: THEIRS } as const,
      { t: 'follow', card: THEIRS, mode: 'copy' } as const,
    ]) {
      expect(describeForLog(a, s, v, false)).not.toContain(cardLabel(THEIRS));
      expect(describeForLog(a, s, v, true)).toContain(cardLabel(THEIRS));
    }
  });

  it('still names a face-up play', () => {
    const a = { t: 'follow', card: THEIRS, mode: 'surpass' } as const;
    expect(describeForLog(a, s, v, false)).toContain(cardLabel(THEIRS));
  });
});
