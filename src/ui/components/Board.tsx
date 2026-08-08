/**
 * The map of the Reach, laid out like the printed board.
 *
 * The printed map is a wheel: a dark void at the centre carrying the wordmark,
 * a ring of six numbered **gates** around it (1 at the top, 2-6 clockwise), and
 * six pie **wedges** radiating outward, one per cluster, each holding that
 * cluster's three planets. Planets are discs with their building slots drawn as
 * outlined triangles and their resource type on a badge beside them.
 *
 * Geometry comes from the engine's graph, not from a picture, so replacing the
 * map data moves the board with it (see docs/DATA-GAPS.md §2). What is copied
 * from the printed board is the *arrangement* — the wheel, the gate ring, the
 * numbering, the triangles — rather than any artwork.
 */
import {
  CLUSTER_COUNT,
  clusterOf,
  isGate,
  type GameState,
  type VariantDef,
} from '../../engine';
import { PLAYER_COLORS, PLAYER_NAMES } from '../describe';
import { BuildingSlot, CLUSTER_TINT, RESOURCE_COLOR, ResourceGlyph, Ship } from './Glyphs';

const SIZE = 680;
const C = SIZE / 2;
const VOID_R = 88; // the dark centre
const GATE_IN = 92; // gate ring inner edge
const GATE_OUT = 146; // gate ring outer edge
const PLANET_R = 286; // outer edge of the planet field

/** Cluster c occupies the wedge centred on this angle, with gate 1 at the top. */
function clusterAngle(cluster: number): number {
  return (cluster * (360 / CLUSTER_COUNT) - 90) * (Math.PI / 180);
}

const WEDGE = (2 * Math.PI) / CLUSTER_COUNT;

/**
 * Where each of a cluster's three planets sits inside its wedge, as
 * (fraction of the wedge's half-angle, fraction of the radial span).
 *
 * The printed board scatters them rather than lining them up, which is what
 * makes a cluster read as a place instead of a row. This keeps that character
 * while staying regular enough that six clusters do not look accidental.
 */
const PLANET_SLOTS: [number, number][] = [
  [-0.62, 0.34],
  [0.06, 0.98],
  [0.66, 0.42],
];

/** Screen position of a system. Exported so the UI can point at systems. */
export function systemPos(v: VariantDef, id: number): { x: number; y: number } {
  const def = v.systems[id];
  const base = clusterAngle(def.cluster);
  if (def.kind === 'gate') {
    const r = (GATE_IN + GATE_OUT) / 2;
    return { x: C + r * Math.cos(base), y: C + r * Math.sin(base) };
  }
  const [spread, depth] = PLANET_SLOTS[def.slot - 1];
  const a = base + spread * (WEDGE / 2);
  const r = GATE_OUT + 42 + depth * (PLANET_R - GATE_OUT - 42);
  return { x: C + r * Math.cos(a), y: C + r * Math.sin(a) };
}

/**
 * Labels sit radially outward from their planet, away from the hub. Anything
 * fixed — always above, always below — collides as soon as two planets in a
 * wedge share a radius, which the scatter above guarantees.
 */
function labelPos(v: VariantDef, id: number, discR: number): { x: number; y: number } {
  const { x, y } = systemPos(v, id);
  const d = Math.hypot(x - C, y - C) || 1;
  const k = (discR + 16) / d;
  return { x: x + (x - C) * k, y: y + (y - C) * k + 4 };
}

/** An annular sector — one gate's segment of the ring. */
function ringSector(cluster: number, inner: number, outer: number, pad = 0.035): string {
  const a0 = clusterAngle(cluster) - WEDGE / 2 + pad;
  const a1 = clusterAngle(cluster) + WEDGE / 2 - pad;
  const p = (r: number, a: number) => `${C + r * Math.cos(a)} ${C + r * Math.sin(a)}`;
  return [
    `M ${p(inner, a0)}`,
    `L ${p(outer, a0)}`,
    `A ${outer} ${outer} 0 0 1 ${p(outer, a1)}`,
    `L ${p(inner, a1)}`,
    `A ${inner} ${inner} 0 0 0 ${p(inner, a0)}`,
    'Z',
  ].join(' ');
}

interface Props {
  state: GameState;
  variant: VariantDef;
  /** Systems to ring in white — the targets of the available actions. */
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
      <defs>
        <radialGradient id="voidFill">
          <stop offset="0%" stopColor="#14131c" />
          <stop offset="100%" stopColor="#08070c" />
        </radialGradient>
        <radialGradient id="gateFill">
          <stop offset="0%" stopColor="#2b2a38" />
          <stop offset="100%" stopColor="#1a1a24" />
        </radialGradient>
      </defs>

      {/* Cluster wedges, tinted like the printed rainbow. */}
      {Array.from({ length: CLUSTER_COUNT }, (_, cl) => {
        const dead = state.systems[cl * 4].outOfPlay;
        return (
          <path
            key={cl}
            d={ringSector(cl, GATE_OUT - 4, SIZE / 2 - 6, 0.012)}
            fill={CLUSTER_TINT[cl]}
            opacity={dead ? 0.05 : 0.19}
          />
        );
      })}

      {/* Edges sit under the systems. */}
      {edges.map(([a, b]) => {
        const pa = systemPos(variant, a);
        const pb = systemPos(variant, b);
        const path =
          isGate(a) && isGate(b) && Math.abs(clusterOf(a) - clusterOf(b)) % 5 !== 1;
        return (
          <line
            key={`${a}-${b}`}
            x1={pa.x}
            y1={pa.y}
            x2={pb.x}
            y2={pb.y}
            className={path ? 'edge edge-path' : 'edge'}
          />
        );
      })}

      {/* The void and its wordmark. */}
      <circle cx={C} cy={C} r={VOID_R} fill="url(#voidFill)" stroke="#3a3550" strokeWidth="1.5" />
      <text x={C} y={C + 9} className="board-wordmark">
        ARCS
      </text>

      {/* Gates: numbered segments of the ring. */}
      {Array.from({ length: CLUSTER_COUNT }, (_, cl) => {
        const id = cl * 4;
        const sys = state.systems[id];
        if (sys.outOfPlay) return null;
        const { x, y } = systemPos(variant, id);
        const on = highlight?.has(id);
        return (
          <g
            key={id}
            className={`system${selected === id ? ' system-selected' : ''}`}
            onClick={() => onSelect?.(selected === id ? null : id)}
          >
            <path
              d={ringSector(cl, GATE_IN, GATE_OUT)}
              fill="url(#gateFill)"
              className={on ? 'gate-shape system-highlight' : 'gate-shape'}
            />
            <text x={x} y={y + 7} className="gate-number">
              {cl + 1}
            </text>
            <Pieces state={state} system={id} x={x} y={y + 26} />
          </g>
        );
      })}

      {/* Planets. */}
      {variant.systems.map((def) => {
        if (def.kind !== 'planet' || state.systems[def.id].outOfPlay) return null;
        const sys = state.systems[def.id];
        const { x, y } = systemPos(variant, def.id);
        const r = 34;
        const type = def.planetType!;
        const on = highlight?.has(def.id);

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
              fill={RESOURCE_COLOR[type]}
              fillOpacity={0.32}
              stroke={RESOURCE_COLOR[type]}
              strokeWidth={1.6}
              className={on ? 'planet-shape system-highlight' : 'planet-shape'}
            />

            {/* Building slots: one outlined triangle per slot, filled once built. */}
            {Array.from({ length: def.buildingSlots }, (_, i) => {
              const b = sys.buildings[i];
              const span = (def.buildingSlots - 1) * 16;
              return (
                <BuildingSlot
                  key={i}
                  x={x - span / 2 + i * 16}
                  y={y - 5}
                  size={15}
                  fill={b ? PLAYER_COLORS[b.player] : undefined}
                  kind={b?.kind}
                  damaged={b?.damaged}
                />
              );
            })}

            {/* The resource badge, as printed beside each planet. */}
            <g transform={`translate(${x + r - 6} ${y - r + 1})`}>
              <circle r="10" fill="#0d0c12" stroke={RESOURCE_COLOR[type]} strokeWidth="1.4" />
              <g transform="translate(-7 -7)">
                <ResourceGlyph type={type} size={14} />
              </g>
            </g>

            {(() => {
              const lp = labelPos(variant, def.id, r);
              return (
                <text x={lp.x} y={lp.y} className="system-label">
                  {def.label}
                </text>
              );
            })()}
            <Pieces state={state} system={def.id} x={x} y={y + 22} />
          </g>
        );
      })}
    </svg>
  );
}

/**
 * Ships per player, drawn under a system.
 *
 * Fresh and damaged are separate stacks with their own glyph — upright and
 * tipped over — rather than one count with a modifier, because only fresh ships
 * count for control (p6) and only damaged ones die to the next hit.
 */
function Pieces({
  state,
  system,
  x,
  y,
}: {
  state: GameState;
  system: number;
  x: number;
  y: number;
}) {
  const sys = state.systems[system];
  const stacks: { player: number; n: number; damaged: boolean }[] = [];
  sys.fresh.forEach((n, p) => {
    if (n > 0) stacks.push({ player: p, n, damaged: false });
    if (sys.damaged[p] > 0) stacks.push({ player: p, n: sys.damaged[p], damaged: true });
  });
  if (stacks.length === 0) return null;

  const w = 26;
  return (
    <g transform={`translate(${x - ((stacks.length - 1) * w) / 2} ${y})`}>
      {stacks.map((s, i) => (
        <g key={`${s.player}-${s.damaged}`}>
          <title>
            {`${s.n} ${s.damaged ? 'damaged' : 'fresh'} ship${s.n === 1 ? '' : 's'} — ${PLAYER_NAMES[s.player]}`}
          </title>
          <Ship x={i * w - 6} y={0} size={15} color={PLAYER_COLORS[s.player]} damaged={s.damaged} />
          <text
            x={i * w + 6}
            y={4}
            className={s.damaged ? 'ship-count ship-damaged' : 'ship-count'}
            fill={PLAYER_COLORS[s.player]}
          >
            {s.n}
          </text>
        </g>
      ))}
    </g>
  );
}
