/** The board drawing: what it takes from the map graph, and how it shows damage. */
import { describe, expect, it } from 'vitest';
import { renderToStaticMarkup } from 'react-dom/server';
import { makeVariant, mulberry32, newGame, planetId } from '../src/engine';
import { Board } from '../src/ui/components/Board';

function draw(mutate?: (s: ReturnType<typeof newGame>) => void) {
  const v = makeVariant(3, 0);
  const s = newGame(v, mulberry32(4), 0);
  mutate?.(s);
  return renderToStaticMarkup(<Board state={s} variant={v} />);
}

describe('board', () => {
  it('tells fresh ships from damaged ones', () => {
    const html = draw((s) => {
      // Pick an in-play planet and give one player both kinds of ship.
      const id = s.systems.findIndex((sys, i) => !sys.outOfPlay && i % 4 !== 0);
      s.systems[id].fresh[0] = 2;
      s.systems[id].damaged[0] = 3;
    });
    expect(html).toContain('2 fresh ships');
    expect(html).toContain('3 damaged ships');
    // Damaged ships are tipped over, which is the only difference on the table.
    expect(html).toMatch(/rotate\(74\)/);
    expect(html).toMatch(/rotate\(0\)/);
  });

  it('singularises one ship', () => {
    const html = draw((s) => {
      const id = s.systems.findIndex((sys, i) => !sys.outOfPlay && i % 4 !== 0);
      s.systems[id].fresh[0] = 1;
      s.systems[id].damaged[0] = 0;
    });
    expect(html).toContain('1 fresh ship —');
  });

  it('draws the cross-cluster planet edge when both clusters are in play', () => {
    const v = makeVariant(3, 0);
    const s = newGame(v, mulberry32(4), 0);
    const a = planetId(1, 2);
    const b = planetId(2, 0);
    const live = !s.systems[a].outOfPlay && !s.systems[b].outOfPlay;
    const html = renderToStaticMarkup(<Board state={s} variant={v} />);
    const edges = (html.match(/class="edge"/g) ?? []).length;
    // Every in-play adjacency is drawn once, so the count follows the graph.
    const expected = v.systems.reduce(
      (n, def) =>
        s.systems[def.id].outOfPlay
          ? n
          : n + def.adjacent.filter((x) => x > def.id && !s.systems[x].outOfPlay).length,
      0,
    );
    expect(edges + (html.match(/class="edge edge-path"/g) ?? []).length).toBe(expected);
    if (live) expect(v.systems[a].adjacent).toContain(b);
  });
});
