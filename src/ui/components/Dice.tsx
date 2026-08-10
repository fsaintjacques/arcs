/**
 * The battle dice, drawn as they sit on the table.
 *
 * Each die is a rounded square in its type's colour — red assault, blue
 * skirmish, gold raid — showing the symbols of the face it landed on: a burst
 * for a hit, a red burst for a self-hit, a building for a building hit, a key
 * for a key. The intercept "ring that may enclose other symbols" (aid booklet
 * p3) is drawn exactly as that: a dashed ring inside the die.
 *
 * As with the cards, these are original renderings of the printed symbols'
 * *meaning*, not scans of the artwork.
 */
import type { ReactElement } from 'react';
import { DIE_FACES } from '../../engine/dice';
import type { DieFace, DieType, GameState, VariantDef } from '../../engine/types';
import { PLAYER_COLORS, PLAYER_NAMES } from '../describe';

const DIE_COLOR: Record<DieType, string> = {
  assault: '#c4463a',
  skirmish: '#7ba7c9',
  raid: '#d8a24a',
};

const DIE_LABEL: Record<DieType, string> = {
  assault: 'assault',
  skirmish: 'skirmish',
  raid: 'raid',
};

/** A four-point burst, the hit symbol. */
function Burst({ x, y, size, color }: { x: number; y: number; size: number; color: string }) {
  const s = size / 10;
  return (
    <path
      transform={`translate(${x - size / 2} ${y - size / 2}) scale(${s})`}
      d="M5 0 L6.2 3.8 L10 5 L6.2 6.2 L5 10 L3.8 6.2 L0 5 L3.8 3.8 Z"
      fill={color}
    />
  );
}

/** A building silhouette, the building-hit symbol. */
function BuildingHit({ x, y, size }: { x: number; y: number; size: number }) {
  const h = size * 0.9;
  return (
    <polygon
      points={`${x},${y - h / 2} ${x + size / 2},${y + h / 2} ${x - size / 2},${y + h / 2}`}
      fill="#c9772f"
    />
  );
}

/** A key, the raid symbol. */
function KeyGlyph({ x, y, size }: { x: number; y: number; size: number }) {
  const s = size / 14;
  return (
    <g
      transform={`translate(${x - size / 2} ${y - size / 2}) scale(${s})`}
      stroke="#e8d27f"
      strokeWidth="2.2"
      fill="none"
    >
      <circle cx="7" cy="3.6" r="2.8" />
      <path d="M7 6.4 L7 13 M7 10.4 H10 M7 12.4 H9.2" />
    </g>
  );
}

/** Where 1..3 symbols sit inside a die, as (x, y, size) in a 26px box. */
const LAYOUT: Record<number, [number, number, number][]> = {
  1: [[13, 13, 12]],
  2: [
    [8.6, 13, 9.5],
    [17.4, 13, 9.5],
  ],
  3: [
    [8.6, 9.4, 8.5],
    [17.4, 9.4, 8.5],
    [13, 17.8, 8.5],
  ],
};

/** One rolled die: `face` is an index into `DIE_FACES[type]`. */
export function Die({ type, face }: { type: DieType; face: number }) {
  const f: DieFace = DIE_FACES[type][face];
  const color = DIE_COLOR[type];

  const symbols: ((x: number, y: number, size: number) => ReactElement)[] = [];
  for (let i = 0; i < f.hits; i++) {
    symbols.push((x, y, size) => <Burst key={`h${i}${x}`} x={x} y={y} size={size} color="#f2ede2" />);
  }
  for (let i = 0; i < f.selfHits; i++) {
    symbols.push((x, y, size) => <Burst key={`s${i}${x}`} x={x} y={y} size={size} color="#e05a4f" />);
  }
  for (let i = 0; i < f.buildingHits; i++) {
    symbols.push((x, y, size) => <BuildingHit key={`b${i}${x}`} x={x} y={y} size={size} />);
  }
  for (let i = 0; i < f.keys; i++) {
    symbols.push((x, y, size) => <KeyGlyph key={`k${i}${x}`} x={x} y={y} size={size} />);
  }

  const spots = LAYOUT[Math.min(symbols.length, 3) as 1 | 2 | 3] ?? [];
  const title =
    [
      f.hits ? `${f.hits} hit${f.hits > 1 ? 's' : ''}` : '',
      f.selfHits ? `${f.selfHits} self-hit` : '',
      f.buildingHits ? 'building hit' : '',
      f.keys ? `${f.keys} key${f.keys > 1 ? 's' : ''}` : '',
      f.intercept ? 'intercept' : '',
    ]
      .filter(Boolean)
      .join(' + ') || 'blank';

  return (
    <svg width="26" height="26" viewBox="0 0 26 26" className="die" role="img" aria-label={`${DIE_LABEL[type]} die: ${title}`}>
      <title>{`${DIE_LABEL[type]}: ${title}`}</title>
      <rect x="1" y="1" width="24" height="24" rx="5" fill="#171420" stroke={color} strokeWidth="1.6" />
      {/* The intercept ring encloses whatever else the face shows. */}
      {f.intercept > 0 && (
        <circle cx="13" cy="13" r="9.6" fill="none" stroke={color} strokeWidth="1.3" strokeDasharray="2.4 2" />
      )}
      {symbols.map((draw, i) => {
        const [x, y, size] = spots[i] ?? [13, 13, 10];
        return draw(x, y, size);
      })}
    </svg>
  );
}

/**
 * The battle in progress: who is fighting where, the dice as rolled, and what
 * is still to be resolved. Sits directly above the action list, because that
 * is where the hits get assigned.
 */
export function BattlePanel({ state, variant }: { state: GameState; variant: VariantDef }) {
  const b = state.battle;
  if (!b) return null;

  const anyRolled = b.rolled.assault.length + b.rolled.skirmish.length + b.rolled.raid.length > 0;
  const left = [
    b.hits > 0 ? `${b.hits} hit${b.hits > 1 ? 's' : ''}` : '',
    b.buildingHits > 0 ? `${b.buildingHits} building` : '',
    b.selfHits > 0 ? `${b.selfHits} self-hit${b.selfHits > 1 ? 's' : ''}` : '',
    b.keys > 0 ? `${b.keys} key${b.keys > 1 ? 's' : ''}` : '',
  ]
    .filter(Boolean)
    .join(' · ');

  return (
    <section className="panel battle-panel">
      <h2>Battle</h2>
      <p className="battle-head">
        <span style={{ color: PLAYER_COLORS[b.attacker] }}>{PLAYER_NAMES[b.attacker]}</span>
        {' attacks '}
        <span style={{ color: PLAYER_COLORS[b.defender] }}>{PLAYER_NAMES[b.defender]}</span>
        {' at '}
        {variant.systems[b.system].label}
      </p>
      {anyRolled ? (
        <div className="dice-rows">
          {(['assault', 'skirmish', 'raid'] as DieType[]).map((type) =>
            b.rolled[type].length > 0 ? (
              <div key={type} className="dice-row">
                <span className="dice-type" style={{ color: DIE_COLOR[type] }}>
                  {DIE_LABEL[type]}
                </span>
                {b.rolled[type].map((face, i) => (
                  <Die key={i} type={type} face={face} />
                ))}
              </div>
            ) : null,
          )}
        </div>
      ) : (
        <p className="dim">Rolling…</p>
      )}
      {b.interceptResolved && (
        <p className="hint warn">Intercepted — the defender's fresh ships strike back.</p>
      )}
      {anyRolled && <p className="hint">{left ? `To resolve: ${left}` : 'Nothing left to resolve.'}</p>}
    </section>
  );
}
