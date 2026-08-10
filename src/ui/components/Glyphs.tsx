/**
 * Component iconography, drawn to match the printed game.
 *
 * These are original SVG renderings of the printed symbols, not scans: the card
 * and board artwork is Leder Games' copyright, and this repository is public.
 * What is reproduced here is layout, palette and glyph shape — the things that
 * make the UI legible to someone who owns the game — rather than the artwork.
 *
 * Colours are sampled from the rulebook where a sample was available, and
 * flagged where they are not.
 */
import type { ResourceType, Suit } from '../../engine/types';

/**
 * Action-card suit colours.
 *
 * Aggression and Construction are sampled from the rulebook (p10, p3).
 * Administration and Mobilization are matched to the printed palette by eye —
 * no page rendered them at a size worth sampling — so they are the two values
 * to correct first if they look wrong against a physical card.
 */
export const SUIT_COLOR: Record<Suit, string> = {
  administration: '#2e6b7a', // approximated
  aggression: '#ac292f', // sampled, rulebook p10
  construction: '#d94b2c', // sampled, rulebook p3
  mobilization: '#7a4a8c', // approximated
};

/** The actions each suit's pips may buy, as printed down the card's edge. */
export const SUIT_ACTION_TEXT: Record<Suit, string> = {
  administration: 'Tax, Repair, or Influence',
  aggression: 'Battle, Move, or Secure',
  construction: 'Build or Repair',
  mobilization: 'Move or Influence',
};

/**
 * Court medallion colours, sampled from the card images: the suit rosette in
 * the top-right corner of every Guild card.
 */
export const SUIT_MEDALLION: Record<ResourceType, string> = {
  material: '#9b4f9e',
  fuel: '#a8912a',
  weapon: '#d9564a',
  relic: '#7fbcd6',
  psionic: '#3f6fae',
};

/** Board wedge tints, one per cluster, following the printed map's rainbow. */
export const CLUSTER_TINT = [
  '#6d4a7e',
  '#7e4a5c',
  '#4a6d7e',
  '#7e6b4a',
  '#4a7e63',
  '#5c4a7e',
];

/** A four-pointed star — the action pip, and the divider ornament. */
export function Pip({ size = 10, color = 'currentColor' }: { size?: number; color?: string }) {
  return (
    <svg width={size} height={size} viewBox="0 0 10 10" aria-hidden="true">
      <path d="M5 0 L6.2 3.8 L10 5 L6.2 6.2 L5 10 L3.8 6.2 L0 5 L3.8 3.8 Z" fill={color} />
    </svg>
  );
}

/** The raid-cost key, as printed on the Guild banner. */
export function Key({ size = 13 }: { size?: number }) {
  return (
    <svg width={size} height={size * 1.35} viewBox="0 0 10 14" aria-hidden="true">
      <circle cx="5" cy="3.4" r="2.9" fill="none" stroke="#1a1a1a" strokeWidth="1.6" />
      <path d="M5 6.3 L5 13 M5 10.2 L8 10.2 M5 12 L7.4 12" stroke="#1a1a1a" strokeWidth="1.6" fill="none" />
    </svg>
  );
}

/** The five resource glyphs, as they appear on planets and Court medallions. */
export function ResourceGlyph({
  type,
  size = 16,
  color,
}: {
  type: ResourceType;
  size?: number;
  color?: string;
}) {
  const c = color ?? RESOURCE_COLOR[type];
  const common = { width: size, height: size, viewBox: '0 0 20 20', 'aria-hidden': true } as const;

  switch (type) {
    case 'material': // an isometric cube
      return (
        <svg {...common}>
          <path d="M10 2 L17.5 6 L17.5 14 L10 18 L2.5 14 L2.5 6 Z" fill={c} />
          <path d="M10 2 L17.5 6 L10 10 L2.5 6 Z" fill="#fff" opacity="0.35" />
          <path d="M10 10 L10 18 L2.5 14 L2.5 6 Z" fill="#000" opacity="0.18" />
        </svg>
      );
    case 'fuel': // a stack of drums
      return (
        <svg {...common}>
          <rect x="5" y="4" width="10" height="12" rx="1.4" fill={c} />
          <ellipse cx="10" cy="4.4" rx="5" ry="1.7" fill="#fff" opacity="0.4" />
          <path d="M5 8.5 H15 M5 11.5 H15" stroke="#000" strokeWidth="1" opacity="0.3" />
        </svg>
      );
    case 'weapon': // a rocket
      return (
        <svg {...common}>
          <path d="M10 1.5 C12.6 4.4 13.6 8 13.6 11.6 H6.4 C6.4 8 7.4 4.4 10 1.5 Z" fill={c} />
          <path d="M6.4 11.6 L3.6 15.4 L6.4 14.6 Z M13.6 11.6 L16.4 15.4 L13.6 14.6 Z" fill={c} />
          <path d="M8.4 16 h3.2 l-1.6 2.6 Z" fill={c} opacity="0.7" />
          <circle cx="10" cy="7.2" r="1.7" fill="#fff" opacity="0.8" />
        </svg>
      );
    case 'relic': // a cut gem
      return (
        <svg {...common}>
          <path d="M10 1.6 L17 7.4 L10 18.4 L3 7.4 Z" fill={c} />
          <path d="M10 1.6 L17 7.4 L10 7.4 Z" fill="#fff" opacity="0.45" />
          <path d="M3 7.4 H17 M10 7.4 L10 18.4" stroke="#000" strokeWidth="0.8" opacity="0.25" />
        </svg>
      );
    case 'psionic': // the eye-in-an-oval
      return (
        <svg {...common}>
          <ellipse cx="10" cy="10" rx="6.2" ry="8" fill={c} />
          <circle cx="10" cy="8" r="2.8" fill="#fff" opacity="0.9" />
          <circle cx="10" cy="8" r="1.2" fill="#111" />
          <path d="M10 12.2 L10 16 M7.6 14 H12.4" stroke="#fff" strokeWidth="1.2" opacity="0.8" />
        </svg>
      );
  }
}

/** Planet fill colours by type, matching the printed map's tinting. */
export const RESOURCE_COLOR: Record<ResourceType, string> = {
  material: '#c9772f',
  fuel: '#c9a32f',
  weapon: '#c4463a',
  relic: '#5aa8c4',
  psionic: '#7a5ec4',
};

/**
 * A ship, in the printed game's swept-wing silhouette.
 *
 * Fresh ships stand upright; damaged ones are **tipped over** (p12), which is
 * the whole of the distinction on the table, so it is the whole of it here: the
 * same glyph laid on its side and dimmed. Nothing else marks damage, because
 * nothing else marks it on the board — and a damaged ship is one hit from being
 * destroyed, so the difference has to read at a glance.
 */
export function Ship({
  x,
  y,
  size = 14,
  color,
  damaged,
}: {
  x: number;
  y: number;
  size?: number;
  color: string;
  damaged?: boolean;
}) {
  const s = size / 14;
  return (
    <g
      transform={`translate(${x} ${y}) rotate(${damaged ? 74 : 0}) scale(${s})`}
      opacity={damaged ? 0.72 : 1}
    >
      <path
        d="M0 -7 L4.6 3.2 L1.7 1.6 L1.7 6.4 L-1.7 6.4 L-1.7 1.6 L-4.6 3.2 Z"
        fill={color}
        stroke="#0b0d12"
        strokeWidth={damaged ? 1 : 0.7}
        strokeLinejoin="round"
      />
    </g>
  );
}

/**
 * A building slot, as printed on every planet: an outlined triangle, filled by
 * the owner's colour once built.
 */
export function BuildingSlot({
  x,
  y,
  size = 13,
  fill,
  kind,
  damaged,
}: {
  x: number;
  y: number;
  size?: number;
  fill?: string;
  kind?: 'city' | 'starport';
  damaged?: boolean;
}) {
  const h = size * 0.88;
  const pts = `${x},${y - h / 2} ${x + size / 2},${y + h / 2} ${x - size / 2},${y + h / 2}`;
  return (
    <g opacity={damaged ? 0.45 : 1}>
      <polygon
        points={pts}
        fill={fill ?? 'none'}
        stroke={fill ? '#0b0d12' : 'rgba(255,255,255,0.55)'}
        strokeWidth={fill ? 1 : 1.3}
        strokeLinejoin="round"
      />
      {/* A starport carries a bar; a city is solid. */}
      {fill && kind === 'starport' && (
        <line
          x1={x - size * 0.22}
          y1={y + h * 0.16}
          x2={x + size * 0.22}
          y2={y + h * 0.16}
          stroke="#0b0d12"
          strokeWidth={1.4}
        />
      )}
    </g>
  );
}
