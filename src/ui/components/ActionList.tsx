/**
 * The human player's control: every legal action, grouped and filterable.
 *
 * Arcs' action space is wide — a single Move pip offers every ship count on
 * every edge — so the list is grouped by action kind and can be narrowed by
 * clicking a system on the board.
 */
import { useMemo, useState } from 'react';
import type { Action, GameState, VariantDef } from '../../engine';
import { actionGroup, actionSystem, describeAction } from '../describe';

interface Props {
  actions: Action[];
  state: GameState;
  variant: VariantDef;
  selectedSystem: number | null;
  onPlay: (a: Action) => void;
  disabled: boolean;
}

export function ActionList({ actions, state, variant, selectedSystem, onPlay, disabled }: Props) {
  const [group, setGroup] = useState<string | null>(null);

  const groups = useMemo(() => {
    const map = new Map<string, Action[]>();
    for (const a of actions) {
      if (selectedSystem !== null) {
        const target = actionSystem(a);
        if (target !== null && target !== selectedSystem) continue;
      }
      const g = actionGroup(a);
      const list = map.get(g) ?? [];
      list.push(a);
      map.set(g, list);
    }
    return map;
  }, [actions, selectedSystem]);

  const names = [...groups.keys()];
  const active = group && groups.has(group) ? group : names[0] ?? null;
  const shown = active ? groups.get(active)! : [];

  if (actions.length === 0) {
    return (
      <section className="panel actions-panel">
        <h2>Your move</h2>
        <p className="dim">{disabled ? 'Waiting for the other players…' : 'Nothing to decide.'}</p>
      </section>
    );
  }

  return (
    <section className="panel actions-panel">
      <h2>Your move</h2>
      {selectedSystem !== null && (
        <p className="hint">
          Filtered to {variant.systems[selectedSystem].label}. Click it again to clear.
        </p>
      )}
      <div className="tabs">
        {names.map((n) => (
          <button
            key={n}
            className={n === active ? 'tab active' : 'tab'}
            onClick={() => setGroup(n)}
          >
            {n} <span className="count">{groups.get(n)!.length}</span>
          </button>
        ))}
      </div>
      <ul className="action-list">
        {shown.map((a, i) => (
          <li key={i}>
            <button disabled={disabled} onClick={() => onPlay(a)}>
              {describeAction(a, state, variant)}
            </button>
          </li>
        ))}
      </ul>
    </section>
  );
}
