/** The informed trim keeps the actions a full scan would have chosen. */
import { describe, expect, it } from 'vitest';
import {
  applyAction,
  getPending,
  makeVariant,
  type Action,
  type GameState,
  type VariantDef,
} from '../src/engine';
import {
  defaultWeights,
  generateCandidates,
  makeAgent,
  narrow,
  relativeEvaluate,
} from '../src/agents';
import { playGame } from '../src/sim/runner';

/** 1-ply value of an action (omniscient), or -Infinity if unplayable. */
function plyValue(s: GameState, v: VariantDef, player: number, a: Action): number {
  let after: GameState;
  try {
    after = applyAction(s, v, a);
  } catch {
    return -Infinity;
  }
  return relativeEvaluate(after, v, player, defaultWeights);
}

/** The best value a budget-free 1-ply scan can reach. */
function fullScanBest(s: GameState, v: VariantDef, player: number, actions: Action[]): number {
  let best = -Infinity;
  for (const a of actions) best = Math.max(best, plyValue(s, v, player, a));
  return best;
}

describe('generateCandidates', () => {
  it('is a pure filter with narrow-compatible guarantees', () => {
    const v = makeVariant(3, 1);
    playGame(Array.from({ length: 3 }, () => makeAgent('random+')), {
      players: 3,
      seed: 91,
      setupIndex: 1,
      onDecision: (state) => {
        const node = getPending(state, v);
        if (node.kind !== 'decision' || node.actions.length <= 12) return;
        const picked = generateCandidates(state, v, node.player, node.actions, {
          max: 12,
          weights: defaultWeights,
        });
        const again = generateCandidates(state, v, node.player, node.actions, {
          max: 12,
          weights: defaultWeights,
        });
        expect(picked.length).toBe(12);
        expect(picked).toEqual(again); // deterministic
        const offered = new Set(node.actions);
        for (const a of picked) expect(offered.has(a)).toBe(true); // subset
        // Every kind stays visible, like narrow.
        expect(new Set(picked.map((a) => a.t))).toEqual(new Set(node.actions.map((a) => a.t)));
      },
    });
  });

  it('covers the full-scan argmax far better than blind narrow', () => {
    let nodes = 0;
    let inCandidates = 0;
    let inNarrow = 0;
    const MAX = 12;

    for (let g = 0; g < 6 && nodes < 500; g++) {
      const v = makeVariant(3, g % 6);
      const agents = Array.from({ length: 3 }, () => makeAgent('greedy'));
      playGame(agents, {
        players: 3,
        seed: 500 + g,
        setupIndex: g % 6,
        onDecision: (state, player) => {
          const node = getPending(state, v);
          if (node.kind !== 'decision' || node.actions.length <= MAX) return;
          nodes++;
          // Ties are common (many moves swing nothing), so the fair question
          // is whether the trimmed set can still REACH the best value, not
          // whether it kept one arbitrary tie-winner.
          const best = fullScanBest(state, v, player, node.actions);
          const reaches = (kept: Action[]) =>
            kept.some((a) => plyValue(state, v, player, a) >= best - 1e-9);
          const cands = generateCandidates(state, v, player, node.actions, {
            max: MAX,
            weights: defaultWeights,
          });
          if (reaches(cands)) inCandidates++;
          if (reaches(narrow(node.actions, MAX))) inNarrow++;
        },
      });
    }

    const candRate = inCandidates / nodes;
    const narrowRate = inNarrow / nodes;
    // Recorded for FINDINGS.md — the before/after number the milestone gates on.
    console.log(
      `best-value coverage over ${nodes} wide nodes: candidates ${(candRate * 100).toFixed(1)}%, narrow ${(narrowRate * 100).toFixed(1)}%`,
    );
    expect(nodes).toBeGreaterThanOrEqual(300);
    expect(candRate).toBeGreaterThanOrEqual(0.9);
    expect(candRate).toBeGreaterThan(narrowRate);
  }, 120_000);
});
