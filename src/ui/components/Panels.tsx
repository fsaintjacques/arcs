/** Side panels: ambitions, the Court row, player boards, hand, and the log. */
import {
  AMBITIONS,
  ambitionCount,
  actionCard,
  courtCard,
  markerValue,
  openResourceSlots,
  type GameState,
  type VariantDef,
} from '../../engine';
import {
  AMBITION_COUNTS,
  AMBITION_LABEL,
  PLAYER_COLORS,
  PLAYER_NAMES,
  RESOURCE_ICON,
  SUIT_SHORT,
  cardLabel,
} from '../describe';
import type { LogEntry } from '../useGame';

export function Ambitions({ state, variant }: { state: GameState; variant: VariantDef }) {
  return (
    <section className="panel">
      <h2>Ambitions</h2>
      <table className="ambitions">
        <thead>
          <tr>
            <th>Box</th>
            <th>Worth</th>
            {state.playerStates.map((_, p) => (
              <th key={p} style={{ color: PLAYER_COLORS[p] }}>
                P{p}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {AMBITIONS.map((a) => {
            const markers = state.declared[a];
            const first = markers.reduce(
              (n, i) => n + markerValue(variant.ambitionMarkers, i, state.flipped[i]).first,
              0,
            );
            const second = markers.reduce(
              (n, i) => n + markerValue(variant.ambitionMarkers, i, state.flipped[i]).second,
              0,
            );
            const counts = state.playerStates.map((p) => ambitionCount(p, a));
            const top = Math.max(0, ...counts, state.phantom[a] ?? 0);
            return (
              <tr key={a} className={markers.length ? 'declared' : ''}>
                <td title={AMBITION_COUNTS[a]}>{AMBITION_LABEL[a]}</td>
                <td className="num">{markers.length ? `${first}/${second}` : '—'}</td>
                {counts.map((c, p) => (
                  <td key={p} className={`num${c > 0 && c === top ? ' leading' : ''}`}>
                    {c}
                  </td>
                ))}
              </tr>
            );
          })}
        </tbody>
      </table>
      <p className="hint">
        Available markers:{' '}
        {state.availableMarkers
          .map((i) => {
            const v = markerValue(variant.ambitionMarkers, i, state.flipped[i]);
            return `${v.first}/${v.second}`;
          })
          .join(' · ') || 'none'}
      </p>
    </section>
  );
}

export function Court({ state }: { state: GameState }) {
  return (
    <section className="panel">
      <h2>The Court</h2>
      <div className="court">
        {state.court.map((slot, i) => {
          const card = courtCard(slot.card);
          return (
            <div key={i} className="court-card" title={card.text}>
              <div className="court-name">{card.name}</div>
              <div className="court-meta">
                {card.suit ? `${RESOURCE_ICON[card.suit]} · raid ${card.raidCost}` : 'Vox'}
              </div>
              <div className="agents">
                {slot.agents.map((n, p) =>
                  n > 0 ? (
                    <span key={p} style={{ color: PLAYER_COLORS[p] }}>
                      {'●'.repeat(Math.min(n, 6))}
                    </span>
                  ) : null,
                )}
              </div>
            </div>
          );
        })}
      </div>
    </section>
  );
}

export function Players({
  state,
  humanSeats,
  actor,
}: {
  state: GameState;
  humanSeats: number[];
  actor: number | null;
}) {
  return (
    <section className="panel">
      <h2>Players</h2>
      {state.playerStates.map((p, i) => (
        <div key={i} className={`player${actor === i ? ' on-turn' : ''}`}>
          <div className="player-head">
            <span className="swatch" style={{ background: PLAYER_COLORS[i] }} />
            <strong>
              P{i} {PLAYER_NAMES[i]}
            </strong>
            {humanSeats.includes(i) && <span className="tag">you</span>}
            {state.initiative === i && (
              <span className="tag tag-init">{state.initiativeSeized ? 'seized' : 'initiative'}</span>
            )}
            <span className="power">{p.power} Power</span>
          </div>
          <div className="player-row">
            <span className="dim">Resources</span>
            {p.resources.slice(0, openResourceSlots(p)).map((r, slot) => (
              <span key={slot} className={r ? 'chip' : 'chip empty'}>
                {r ? `${RESOURCE_ICON[r]} ${r}` : '·'}
              </span>
            ))}
          </div>
          <div className="player-row">
            <span className="dim">Trophies</span>
            <span className="chip">{p.trophies.length}</span>
            <span className="dim">Captives</span>
            <span className="chip">{p.captives.length}</span>
            <span className="dim">Guild</span>
            <span className="chip">{p.guildCards.length}</span>
            <span className="dim">Cards</span>
            <span className="chip">{p.hand.length}</span>
          </div>
          {Object.entries(p.outrage).some(([, v]) => v) && (
            <div className="player-row outrage">
              <span className="dim">Outraged</span>
              {Object.entries(p.outrage)
                .filter(([, v]) => v)
                .map(([r]) => (
                  <span key={r} className="chip warn">
                    {r}
                  </span>
                ))}
            </div>
          )}
        </div>
      ))}
    </section>
  );
}

export function Trick({ state }: { state: GameState }) {
  const lead = state.round.lead;
  return (
    <section className="panel">
      <h2>This round</h2>
      {lead ? (
        <p>
          Lead: <strong>{cardLabel(lead.card)}</strong> ({SUIT_SHORT[actionCard(lead.card).suit]}),
          effective number <strong>{state.round.leadNumber}</strong>
          {state.round.leadNumber === 0 && <span className="hint"> — ambition declared</span>}
        </p>
      ) : (
        <p className="dim">No lead card yet.</p>
      )}
      <div className="played">
        {state.round.played.map((c, i) => (
          <span key={i} className="chip" style={{ borderColor: PLAYER_COLORS[c.player] }}>
            P{c.player} {c.faceDown && c.card < 0 ? '??' : cardLabel(c.card)} · {c.mode}
          </span>
        ))}
      </div>
    </section>
  );
}

export function Hand({ state, seat }: { state: GameState; seat: number | null }) {
  if (seat === null) return null;
  const hand = [...state.playerStates[seat].hand].sort(
    (a, b) => actionCard(a).suit.localeCompare(actionCard(b).suit) || actionCard(a).number - actionCard(b).number,
  );
  return (
    <section className="panel">
      <h2>Your hand</h2>
      <div className="hand">
        {hand.map((card) => {
          const c = actionCard(card);
          return (
            <div key={card} className={`card suit-${c.suit}`}>
              <div className="card-number">{c.number}</div>
              <div className="card-suit">{SUIT_SHORT[c.suit]}</div>
              <div className="card-pips">{'●'.repeat(c.pips)}</div>
              {c.ambition && <div className="card-ambition">{c.ambition === 'any' ? 'any' : c.ambition}</div>}
            </div>
          );
        })}
        {hand.length === 0 && <p className="dim">No cards left this chapter.</p>}
      </div>
    </section>
  );
}

export function Log({ log }: { log: LogEntry[] }) {
  return (
    <section className="panel log-panel">
      <h2>Log</h2>
      <ol className="log">
        {log
          .slice()
          .reverse()
          .map((e, i) => (
            <li key={log.length - i}>
              <span className="dim">ch{e.chapter}</span>{' '}
              {e.player !== null && (
                <span style={{ color: PLAYER_COLORS[e.player] }}>P{e.player}</span>
              )}{' '}
              {e.text}
            </li>
          ))}
        {log.length === 0 && <li className="dim">Nothing yet.</li>}
      </ol>
    </section>
  );
}
