/**
 * Determinized Information-Set MCTS (ISMCTS) with max^n backup.
 *
 * Arcs is an imperfect-information game with more than two players, which
 * rules out plain UCT on a shared tree and rules out minimax backup:
 *
 *   - **Determinization**: every iteration samples a world consistent with the
 *     agent's observation (`determinize`) and searches that. Nodes are keyed by
 *     the action sequence from the root, so statistics pool across worlds —
 *     this is the "single-observer ISMCTS" formulation.
 *   - **max^n backup**: each node carries a value *per seat*, and selection at
 *     a node maximises the value of the seat to move there. With 3-4 players
 *     that is the honest generalisation of UCT; assuming the opponents
 *     minimise the root player's score would model a coalition that is not in
 *     the game.
 *   - **Chance nodes** are resolved from the RNG as encountered, so the tree
 *     is open-loop across dice: children of a chance node pool over outcomes,
 *     which is what makes a fixed iteration budget usable at this branching
 *     factor.
 */
import { applyActionMut, determinize, encodeAction, getPending, resolveChanceMut } from '../engine';
import type { Action, GameState, RNG, VariantDef } from '../engine';
import { generateCandidates } from './candidates';
import { defaultWeights, type Weights } from './eval';
import { narrow, rollout, terminalVector, valueVector } from './rollout';
import type { Agent } from './types';

export interface MctsOpts {
  /** Search iterations per decision. */
  iterations: number;
  /** Exploration constant of UCT. */
  c: number;
  /** Decisions played per rollout before the heuristic values the position. */
  rolloutDepth: number;
  /** Cap on branching at any one node (see `narrow`). */
  maxActions: number;
  /** Tree depth in decisions; beyond this the rollout policy takes over. */
  maxDepth: number;
  /** Trim wide nodes with `generateCandidates` instead of blind `narrow`. */
  candidates: boolean;
  weights: Partial<Weights>;
}

interface Node {
  /** Seat to move at this node. */
  player: number;
  visits: number;
  /** Total backed-up value per seat. */
  totals: number[];
  children: Map<string, Node>;
  action: Action | null;
}

function newNode(player: number, action: Action | null, players: number): Node {
  return {
    player,
    visits: 0,
    totals: Array(players).fill(0),
    children: new Map(),
    action,
  };
}

const key = encodeAction;

/**
 * UCT over the children that are legal *in this determinization*. Availability
 * varies between iterations, so the parent visit count is replaced by the sum
 * over the currently-available children — the standard ISMCTS correction that
 * stops rarely-legal actions from looking unexplored forever.
 */
function uctChild(node: Node, available: Node[], c: number): Node | null {
  let best: Node | null = null;
  let bestScore = -Infinity;
  const total = available.reduce((n, child) => n + child.visits, 0);
  for (const child of available) {
    if (child.visits === 0) return child;
    const exploit = child.totals[node.player] / child.visits;
    const explore = c * Math.sqrt(Math.log(Math.max(2, total)) / child.visits);
    const score = exploit + explore;
    if (score > bestScore) {
      bestScore = score;
      best = child;
    }
  }
  return best;
}

export function makeMcts(opts: Partial<MctsOpts> = {}, name = 'mcts'): Agent {
  const iterations = opts.iterations ?? 300;
  const c = opts.c ?? 0.9;
  const rolloutDepth = opts.rolloutDepth ?? 30;
  const maxActions = opts.maxActions ?? 12;
  const maxDepth = opts.maxDepth ?? 12;
  const candidates = opts.candidates ?? false;
  const w: Weights = { ...defaultWeights, ...opts.weights };

  return {
    name,
    choose(obs, actions, ctx) {
      const v = ctx.variant;
      if (actions.length === 1) return actions[0];

      const root = newNode(ctx.player, null, obs.state.players);

      for (let iter = 0; iter < iterations; iter++) {
        const state = determinize(obs, v, ctx.rng);
        descend(root, state, v, ctx.rng, 0);
      }

      // Play the most-visited child: robust to a single lucky evaluation.
      let best: Action = actions[0];
      let bestVisits = -1;
      for (const child of root.children.values()) {
        if (child.visits > bestVisits) {
          bestVisits = child.visits;
          best = child.action!;
        }
      }
      return best;

      function descend(node: Node, state: GameState, v2: VariantDef, rng: RNG, depth: number): number[] {
        // Resolve any chance the position owes before looking at the node.
        for (let guard = 0; guard < 64; guard++) {
          const n = getPending(state, v2);
          if (n.kind !== 'chance') break;
          resolveChanceMut(state, v2, rng); // the state is ours, mutate in place
        }

        const pending = getPending(state, v2);
        if (pending.kind !== 'decision') {
          const value = pending.kind === 'over' ? terminalVector(state) : valueVector(state, v2, w);
          backup(node, value);
          return value;
        }
        if (depth >= maxDepth) {
          const value = rollout(state, v2, rng, { depth: rolloutDepth, weights: w });
          backup(node, value);
          return value;
        }

        node.player = pending.player;

        // Legality is re-derived every visit: this determinization's dice and
        // hands may differ from the one that first expanded this node.
        const legal = candidates
          ? generateCandidates(state, v2, pending.player, pending.actions, { max: maxActions, weights: w })
          : narrow<Action>(pending.actions, maxActions);
        const untried: Action[] = [];
        const available: Node[] = [];
        for (const a of legal) {
          const child = node.children.get(key(a));
          if (child) available.push(child);
          else untried.push(a);
        }

        if (untried.length > 0) {
          const action = untried[Math.floor(rng() * untried.length)];
          const child = newNode(pending.player, action, state.players);
          node.children.set(key(action), child);
          applyActionMut(state, v2, action);
          const value = rollout(state, v2, rng, { depth: rolloutDepth, weights: w });
          backup(child, value);
          backup(node, value);
          return value;
        }

        const chosen = uctChild(node, available, c);
        if (!chosen) {
          const value = valueVector(state, v2, w);
          backup(node, value);
          return value;
        }
        applyActionMut(state, v2, chosen.action!);
        const value = descend(chosen, state, v2, rng, depth + 1);
        backup(node, value);
        return value;
      }
    },
  };
}

function backup(node: Node, value: number[]): void {
  node.visits++;
  for (let i = 0; i < value.length; i++) node.totals[i] += value[i];
}
