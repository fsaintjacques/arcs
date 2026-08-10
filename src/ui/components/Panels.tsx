/** Side panels: ambitions, the Court row, player boards, hand, and the log. */
import { useState } from 'react';
import { markerValue } from '../../engine/ambitions';
import { actionCard } from '../../engine/cards';
import { openResourceSlots } from '../../engine/playerBoard';
import { AMBITIONS, type GameState, type VariantDef } from '../../engine/types';
import {
  AMBITION_COUNTS,
  AMBITION_LABEL,
  PLAYER_COLORS,
  PLAYER_NAMES,
  RESOURCE_ICON,
} from '../describe';
import type { LogEntry } from '../useGame';
import { ActionCardFace, CourtCardFace } from './Cards';

export function Ambitions({
  state,
  variant,
  counts: tallies,
}: {
  state: GameState;
  variant: VariantDef;
  /** Per-seat tallies in `AMBITIONS` order, counted by the engine. */
  counts: number[][];
}) {
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
          {AMBITIONS.map((a, ai) => {
            const markers = state.declared[a];
            const first = markers.reduce(
              (n, i) => n + markerValue(variant.ambitionMarkers, i, state.flipped[i]).first,
              0,
            );
            const second = markers.reduce(
              (n, i) => n + markerValue(variant.ambitionMarkers, i, state.flipped[i]).second,
              0,
            );
            const counts = state.playerStates.map((_, p) => tallies[p]?.[ai] ?? 0);
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
        {state.court.map((slot, i) => (
          <CourtCardFace key={i} card={slot.card} agents={slot.agents} compact />
        ))}
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
  // Guild cards sit face-up in their owner's play area, so this reveals nothing
  // a player at the table could not already read off it.
  const [openGuild, setOpenGuild] = useState<number | null>(null);
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
            {p.guildCards.length > 0 ? (
              <button
                type="button"
                className={`chip chip-button${openGuild === i ? ' chip-open' : ''}`}
                aria-expanded={openGuild === i}
                onClick={() => setOpenGuild(openGuild === i ? null : i)}
                title={`Show the ${p.guildCards.length} Guild card${p.guildCards.length === 1 ? '' : 's'} P${i} holds`}
              >
                {p.guildCards.length} {openGuild === i ? '▾' : '▸'}
              </button>
            ) : (
              <span className="chip">0</span>
            )}
            <span className="dim">Cards</span>
            <span className="chip">{p.hand.length}</span>
          </div>
          {openGuild === i && (
            <div className="guild-cards">
              {p.guildCards.map((card) => (
                <CourtCardFace key={card} card={card} compact />
              ))}
            </div>
          )}
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
  const zeroed = state.round.leadNumber === 0;
  return (
    <section className="panel">
      <h2>This round</h2>
      {state.round.played.length === 0 && <p className="dim">No cards played yet.</p>}
      <div className="played">
        {state.round.played.map((c, i) => {
          const isLead = lead !== null && c === lead;
          const hidden = c.faceDown && c.card < 0;
          return (
            <figure key={i} className={`play${isLead ? ' play-lead' : ''}`}>
              {hidden ? (
                <div className="card-back" title="Played face down" />
              ) : (
                <ActionCardFace card={c.card} small zeroed={isLead && zeroed} />
              )}
              <figcaption style={{ color: PLAYER_COLORS[c.player] }}>
                P{c.player}
                <span className="dim"> {isLead ? 'lead' : c.mode}</span>
              </figcaption>
            </figure>
          );
        })}
      </div>
      {lead && (
        <p className="hint">
          Follow with {actionCard(lead.card).suit} above{' '}
          <strong>{state.round.leadNumber}</strong> to Surpass
          {zeroed && ' — the zero marker makes any card of the suit enough'}.
        </p>
      )}
    </section>
  );
}

export function Hand({ state, seat }: { state: GameState; seat: number | null }) {
  if (seat === null) return null;
  const hand = [...state.playerStates[seat].hand].sort(
    (a, b) =>
      actionCard(a).suit.localeCompare(actionCard(b).suit) ||
      actionCard(a).number - actionCard(b).number,
  );
  return (
    <section className="panel">
      <h2>Your hand</h2>
      <div className="hand">
        {hand.map((card) => (
          <ActionCardFace key={card} card={card} />
        ))}
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
