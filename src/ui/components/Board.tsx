/**
 * The map of the Reach: 6 clusters on a ring, each a gate plus 3 planets.
 *
 * Laid out from the engine's graph rather than a picture, so it stays correct
 * if the map data is replaced (see docs/DATA-GAPS.md §2).
 */
import { clusterOf, isGate, type GameState, type ResourceType, type VariantDef } from '../../engine';
import { PLAYER_COLORS, RESOURCE_ICON } from '../describe';

const SIZE = 620;
const CENTRE = SIZE / 2;
const GATE_R = 132;
const PLANET_R = 238;

const PLANET_COLORS: Record<ResourceType, string> = {
  material: '#c98b3f',
  fuel: '#c9a83f',
  weapon: '#c04a3f',
  relic: '#4aa9c0',
  psionic: '#8a5fc0',
};

/** Screen position of a system, from its cluster and slot. */
export function systemPos(v: VariantDef, id: number): { x: number; y: number } {
  const def = v.systems[id];
  const base = (def.cluster * 60 - 90) * (Math.PI / 180);
  if (def.kind === 'gate') {
    return { x: CENTRE + GATE_R * Math.cos(base), y: CENTRE + GATE_R * Math.sin(base) };
  }
  const spread = ((def.slot - 2) * 20 * Math.PI) / 180;
  const a = base + spread;
  return { x: CENTRE + PLANET_R * Math.cos(a), y: CENTRE + PLANET_R * Math.sin(a) };
}

interface Props {
  state: GameState;
  variant: VariantDef;
  /** Systems to ring in white — the targets of the hovered/available actions. */
  highlight?: Set<number>;
  selected?: number | null;
  onSelect?: (system: number | null) => void;
}

export function Board({ state, variant, highlight, selected, onSelect }: Props) {
  const edges: [number, number][] = [];
  for (const def of variant.systems) {
    if (state.systems[def.id].outOfPlay) continue;
    for (const n of def.adjacent) {
      if (n > def.id && !state.systems[n].outOfPlay) edges.push([def.id, n]);
    }
  }

  return (
    <svg className="board" viewBox={`0 0 ${SIZE} ${SIZE}`} role="img" aria-label="Map of the Reach">
      <circle cx={CENTRE} cy={CENTRE} r={62} className="board-core" />
      <text x={CENTRE} y={CENTRE + 5} className="board-core-label">
        ARCS
      </text>

      {edges.map(([a, b]) => {
        const pa = systemPos(variant, a);
        const pb = systemPos(variant, b);
        // Gate-to-gate links across a dead cluster are path markers.
        const isPath =
          isGate(a) && isGate(b) && Math.abs(clusterOf(a) - clusterOf(b)) % 5 !== 1;
        return (
          <line
            key={`${a}-${b}`}
            x1={pa.x}
            y1={pa.y}
            x2={pb.x}
            y2={pb.y}
            className={isPath ? 'edge edge-path' : 'edge'}
          />
        );
      })}

      {variant.systems.map((def) => {
        const sys = state.systems[def.id];
        if (sys.outOfPlay) return null;
        const { x, y } = systemPos(variant, def.id);
        const gate = def.kind === 'gate';
        const r = gate ? 26 : 30;
        const fill = gate ? '#2b3348' : PLANET_COLORS[def.planetType!];

        return (
          <g
            key={def.id}
            className={`system${selected === def.id ? ' system-selected' : ''}`}
            onClick={() => onSelect?.(selected === def.id ? null : def.id)}
          >
            <circle
              cx={x}
              cy={y}
              r={r}
              fill={fill}
              className={highlight?.has(def.id) ? 'system-shape system-highlight' : 'system-shape'}
            />
            <text x={x} y={y - r - 7} className="system-label">
              {gate ? def.label : `${def.label} ${RESOURCE_ICON[def.planetType!]}`}
            </text>

            {/* Buildings: a square per starport, a house shape per city. */}
            {sys.buildings.map((b, i) => (
              <rect
                key={i}
                x={x - r + 5 + i * 13}
                y={y - 6}
                width={11}
                height={11}
                rx={b.kind === 'city' ? 1 : 5}
                fill={PLAYER_COLORS[b.player]}
                opacity={b.damaged ? 0.4 : 1}
                stroke="#0d1017"
                strokeWidth={1}
              />
            ))}

            {/* Ship counts per player, fresh + damaged. */}
            {sys.fresh.map((fresh, p) => {
              const damaged = sys.damaged[p];
              if (fresh + damaged === 0) return null;
              const order = sys.fresh
                .map((f, i) => (f + sys.damaged[i] > 0 ? i : -1))
                .filter((i) => i >= 0)
                .indexOf(p);
              return (
                <text
                  key={p}
                  x={x - r + 8 + order * 16}
                  y={y + r - 4}
                  className="ship-count"
                  fill={PLAYER_COLORS[p]}
                >
                  {fresh}
                  {damaged > 0 ? `+${damaged}` : ''}
                </text>
              );
            })}
          </g>
        );
      })}
    </svg>
  );
}
