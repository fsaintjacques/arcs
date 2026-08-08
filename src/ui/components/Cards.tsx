/**
 * Card faces, laid out like the printed cards.
 *
 * Three faces, and the printed ones differ structurally rather than
 * decoratively, so they are three components rather than one with props:
 *
 *   - **Action** — white, the suit's colour as ink. Numeral and pips in the
 *     top-left, the suit and its actions set vertically up the spine, art to
 *     the right. Cards 2-6 carry an ambition symbol; the "1" declares nothing
 *     and the "7" declares anything.
 *   - **Guild** — gold frame, raid-cost keys on a banner top-left, the suit
 *     rosette top-right, arched art, name plate, rules text, `BC | GUILD | nn`.
 *   - **Vox** — the same frame with no rosette and no keys; the name sits at
 *     the top, the art below the text, and the footer reads `VOX`.
 *
 * The artwork itself is a stylised stand-in — see the note in Glyphs.tsx.
 */
import { actionCard, courtCard, type AmbitionId } from '../../engine';
import { AMBITION_LABEL } from '../describe';
import { Key, Pip, ResourceGlyph, SUIT_ACTION_TEXT, SUIT_COLOR, SUIT_MEDALLION } from './Glyphs';

// ---------------------------------------------------------------------------
// Action cards
// ---------------------------------------------------------------------------

/** The ambition symbol printed at the top of a "2"-"7". */
function AmbitionMark({ ambition, color }: { ambition: AmbitionId | 'any'; color: string }) {
  const label = ambition === 'any' ? 'Any' : AMBITION_LABEL[ambition];
  return (
    <div className="ac-ambition" style={{ borderColor: color, color }}>
      {label}
    </div>
  );
}

/**
 * Stylised art panel. Each suit gets its own motif so cards are tellable apart
 * at a glance, the way the printed duotone illustrations are.
 */
function ArtPanel({ suit, seed }: { suit: string; seed: number }) {
  const color = SUIT_COLOR[suit as keyof typeof SUIT_COLOR];
  // A deterministic scatter, so a given card always draws the same way.
  const rand = (n: number) => {
    const x = Math.sin(seed * 97.13 + n * 31.7) * 10000;
    return x - Math.floor(x);
  };
  return (
    <svg className="ac-art" viewBox="0 0 60 100" preserveAspectRatio="none" aria-hidden="true">
      <rect width="60" height="100" fill={color} opacity="0.9" />
      {suit === 'construction' &&
        Array.from({ length: 7 }, (_, i) => (
          <rect
            key={i}
            x={6 + rand(i) * 34}
            y={14 + rand(i + 20) * 66}
            width={10 + rand(i + 40) * 12}
            height={8 + rand(i + 60) * 14}
            fill="none"
            stroke="#fff"
            strokeWidth="1.1"
            opacity="0.85"
          />
        ))}
      {suit === 'aggression' &&
        Array.from({ length: 9 }, (_, i) => (
          <path
            key={i}
            d={`M${8 + rand(i) * 44} ${10 + rand(i + 20) * 78} l${5 + rand(i + 40) * 9} ${-9 - rand(i + 60) * 10}`}
            stroke="#fff"
            strokeWidth="1.4"
            opacity="0.85"
            strokeLinecap="round"
          />
        ))}
      {suit === 'administration' &&
        Array.from({ length: 8 }, (_, i) => (
          <circle
            key={i}
            cx={10 + rand(i) * 40}
            cy={12 + rand(i + 20) * 74}
            r={3 + rand(i + 40) * 6}
            fill="none"
            stroke="#fff"
            strokeWidth="1.1"
            opacity="0.8"
          />
        ))}
      {suit === 'mobilization' &&
        Array.from({ length: 7 }, (_, i) => (
          <path
            key={i}
            d={`M4 ${14 + i * 12} q 26 ${-8 - rand(i) * 12} 52 ${2 + rand(i + 20) * 6}`}
            fill="none"
            stroke="#fff"
            strokeWidth="1.1"
            opacity="0.75"
          />
        ))}
    </svg>
  );
}

export function ActionCardFace({
  card,
  zeroed,
  small,
}: {
  card: number;
  /** The zero marker sits over the number once an ambition is declared on it. */
  zeroed?: boolean;
  small?: boolean;
}) {
  const c = actionCard(card);
  const color = SUIT_COLOR[c.suit];

  return (
    <div className={`action-card${small ? ' action-card-sm' : ''}`} style={{ '--suit': color } as React.CSSProperties}>
      {c.ambition && !small && <AmbitionMark ambition={c.ambition} color={color} />}
      <div className="ac-body">
        <div className="ac-spine">
          {zeroed ? (
            <div className="ac-zero" title="Ambition declared — this card counts as 0">
              <strong>0</strong>
              <span>Ambition declared</span>
            </div>
          ) : (
            <div className="ac-number">{c.number}</div>
          )}
          <div className="ac-pips">
            {Array.from({ length: c.pips }, (_, i) => (
              <Pip key={i} size={small ? 7 : 11} color={color} />
            ))}
          </div>
          <div className="ac-suit">
            <span className="ac-suit-name">{c.suit}</span>
            {!small && <span className="ac-suit-actions">{SUIT_ACTION_TEXT[c.suit]}</span>}
          </div>
        </div>
        <ArtPanel suit={c.suit} seed={card} />
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Court cards
// ---------------------------------------------------------------------------

/** The eight-pointed rosette carrying a Guild card's suit. */
function Medallion({ suit, size = 42 }: { suit: keyof typeof SUIT_MEDALLION; size?: number }) {
  const color = SUIT_MEDALLION[suit];
  const points = Array.from({ length: 8 }, (_, i) => {
    const a = (i * 45 - 90) * (Math.PI / 180);
    return `${50 + 48 * Math.cos(a)},${50 + 48 * Math.sin(a)}`;
  }).join(' ');
  return (
    <div className="cc-medallion" style={{ width: size, height: size }}>
      <svg viewBox="0 0 100 100" aria-hidden="true">
        <polygon points={points} fill="#fdfbf4" stroke={color} strokeWidth="7" strokeLinejoin="round" />
        <circle cx="50" cy="50" r="34" fill="#fdfbf4" stroke={color} strokeWidth="3" />
      </svg>
      <span className="cc-medallion-glyph">
        <ResourceGlyph type={suit} size={size * 0.42} />
      </span>
    </div>
  );
}

export function CourtCardFace({
  card,
  agents,
  compact,
}: {
  card: number;
  /** Agents on the card in the Court, per player. */
  agents?: number[];
  compact?: boolean;
}) {
  const c = courtCard(card);
  const vox = c.kind === 'vox';

  return (
    <article className={`court-card${vox ? ' court-vox' : ''}${compact ? ' court-compact' : ''}`}>
      {!vox && (
        <>
          <div className="cc-keys" title={`Costs ${c.raidCost} key${c.raidCost === 1 ? '' : 's'} to raid`}>
            {Array.from({ length: c.raidCost }, (_, i) => (
              <Key key={i} size={compact ? 9 : 12} />
            ))}
          </div>
          <Medallion suit={c.suit!} size={compact ? 30 : 44} />
        </>
      )}

      {vox ? (
        <>
          <h3 className="cc-name cc-name-top">{c.name}</h3>
          <div className="cc-rule" />
          <p className="cc-text">{c.text}</p>
          <div className="cc-art cc-art-vox" aria-hidden="true" />
        </>
      ) : (
        <>
          <div className="cc-art" aria-hidden="true" />
          <h3 className="cc-name">{c.name}</h3>
          <div className="cc-rule" />
          <p className="cc-text">{c.text}</p>
        </>
      )}

      <footer className="cc-foot">
        <span>BC</span>
        <span className="cc-kind">{vox ? 'VOX' : 'GUILD'}</span>
        <span>{String(c.number).padStart(2, '0')}</span>
      </footer>

      {agents && agents.some((n) => n > 0) && (
        <div className="cc-agents">
          {agents.map((n, p) =>
            n > 0 ? (
              <span key={p} className={`agent-pip p${p}`} title={`${n} agent${n === 1 ? '' : 's'}`}>
                {n}
              </span>
            ) : null,
          )}
        </div>
      )}
    </article>
  );
}
