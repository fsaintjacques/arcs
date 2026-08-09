import { useMemo, useState } from 'react';
import { agentNames } from '../agents';
import { actionCard, standings, POWER_THRESHOLD, type GameState } from '../engine';
import { Board } from './components/Board';
import { ActionList } from './components/ActionList';
import { Ambitions, Court, Hand, Log, Players, Trick } from './components/Panels';
import { SimPanel } from './components/SimPanel';
import { actionSystem, PLAYER_NAMES, phaseLabel } from './describe';
import { useGame, type GameConfig } from './useGame';

type Tab = 'play' | 'watch' | 'simulate';

const DEFAULT_PLAY: GameConfig = {
  players: 3,
  seats: [null, 'greedy', 'greedy'],
  seed: 1,
  setupIndex: 0,
  botDelay: 220,
};

const DEFAULT_WATCH: GameConfig = {
  players: 3,
  seats: ['greedy', 'greedy', 'mc'],
  seed: 1,
  setupIndex: 0,
  botDelay: 320,
};

export default function App() {
  const [tab, setTab] = useState<Tab>('play');
  return (
    <div className="app">
      <header>
        <h1>
          Arcs <span className="dim">Lab</span>
        </h1>
        <nav>
          {(['play', 'watch', 'simulate'] as Tab[]).map((t) => (
            <button key={t} className={tab === t ? 'tab active' : 'tab'} onClick={() => setTab(t)}>
              {t[0].toUpperCase() + t.slice(1)}
            </button>
          ))}
        </nav>
      </header>
      {tab === 'simulate' ? <SimPanel /> : <GameView key={tab} mode={tab} />}
      <footer className="dim">
        Unofficial fan-made implementation of <em>Arcs</em> (Cole Wehrle, Leder Games) for strategy
        research. All 31 Court card abilities, the whole map and all 12 setup cards are the printed
        ones; two small component values are still reconstructed — see docs/DATA-GAPS.md.
      </footer>
    </div>
  );
}

function GameView({ mode }: { mode: 'play' | 'watch' }) {
  const game = useGame(mode === 'play' ? DEFAULT_PLAY : DEFAULT_WATCH);
  const [selected, setSelected] = useState<number | null>(null);

  const highlight = useMemo(() => {
    const set = new Set<number>();
    for (const a of game.actions) {
      const s = actionSystem(a);
      if (s !== null) set.add(s);
    }
    return set;
  }, [game.actions]);

  const s = game.state;
  const over = s.phase === 'over';
  const table = over ? standings(s) : [];
  const humanSeat = game.humanSeats[0] ?? null;

  return (
    <div className="game">
      <div className="left">
        <section className="panel status">
          <div className="status-line">
            <strong>Chapter {s.chapter}</strong>
            <span className="dim">of {game.variant.maxChapters}</span>
            <span className="sep">·</span>
            {s.phase === 'actions' && s.turn ? <PipTrack state={s} /> : <span>{phaseLabel(s)}</span>}
            {game.actor !== null && (
              <>
                <span className="sep">·</span>
                <span>
                  {PLAYER_NAMES[game.actor]} to move{game.busy ? ' (thinking…)' : ''}
                </span>
              </>
            )}
            {mode === 'play' && (
              <button
                type="button"
                className="undo-btn"
                onClick={game.undo}
                disabled={!game.canUndo}
                title="Rewind to your previous decision"
              >
                ↶ Undo
              </button>
            )}
          </div>
          <div className="status-line dim">
            {POWER_THRESHOLD[s.players]} Power ends the game.
          </div>
          <Controls game={game} mode={mode} />
        </section>

        <Board
          state={s}
          variant={game.variant}
          highlight={highlight}
          selected={selected}
          onSelect={setSelected}
        />

        {mode === 'play' && <Hand state={s} seat={humanSeat} />}
        <Court state={s} />

        {over && (
          <section className="panel">
            <h2>Final standings</h2>
            <ol className="standings">
              {table.map((row) => (
                <li key={row.player}>
                  {PLAYER_NAMES[row.player]} — <strong>{row.power}</strong> Power
                  {row.rank === 0 && <span className="tag tag-init">winner</span>}
                </li>
              ))}
            </ol>
          </section>
        )}
      </div>

      <div className="right">
        {mode === 'play' && (
          <ActionList
            actions={game.actions}
            state={s}
            variant={game.variant}
            selectedSystem={selected}
            onPlay={game.play}
            disabled={game.busy}
          />
        )}
        <Trick state={s} />
        <Ambitions state={s} variant={game.variant} />
        <Players state={s} humanSeats={game.humanSeats} actor={game.actor} />
        <Log log={game.log} />
      </div>
    </div>
  );
}

/**
 * The turn's pips as a row of circles: one per pip on the played card, and a
 * filled centre once that pip has been spent. Copy and Pivot grant a single
 * action, so they show one circle.
 */
function PipTrack({ state }: { state: GameState }) {
  const turn = state.turn!;
  const cardPips =
    turn.mode === 'lead' || turn.mode === 'surpass' ? actionCard(turn.card).pips : 1;
  const total = Math.max(cardPips, turn.pipsLeft);
  const spent = total - turn.pipsLeft;
  return (
    <span className="pip-track" title={`${turn.pipsLeft} of ${total} pips left`}>
      Actions
      {Array.from({ length: total }, (_, i) => (
        <svg key={i} width="12" height="12" viewBox="0 0 12 12" className="pip-circle" aria-hidden="true">
          <circle cx="6" cy="6" r="4.5" fill="none" stroke="currentColor" strokeWidth="1.4" />
          {i < spent && <circle cx="6" cy="6" r="2.3" fill="currentColor" />}
        </svg>
      ))}
      {turn.freeActions.length > 0 && (
        <span className="dim">+{turn.freeActions.length} free</span>
      )}
    </span>
  );
}

function Controls({ game, mode }: { game: ReturnType<typeof useGame>; mode: 'play' | 'watch' }) {
  const { config, reset } = game;
  const setSeat = (i: number, value: string) => {
    reset({ seats: config.seats.map((s, j) => (i === j ? (value === 'human' ? null : value) : s)) });
  };

  return (
    <div className="controls">
      <label>
        Players
        <select
          value={config.players}
          onChange={(e) => {
            const n = Number(e.target.value);
            const seats = Array.from({ length: n }, (_, i) =>
              config.seats[i] !== undefined ? config.seats[i] : 'greedy',
            );
            if (mode === 'play') seats[0] = null;
            reset({ players: n, seats });
          }}
        >
          {[2, 3, 4].map((n) => (
            <option key={n} value={n}>
              {n}
            </option>
          ))}
        </select>
      </label>

      {config.seats.map((seat, i) => (
        <label key={i}>
          P{i}
          <select value={seat ?? 'human'} onChange={(e) => setSeat(i, e.target.value)}>
            {mode === 'play' && <option value="human">human</option>}
            {agentNames().map((n) => (
              <option key={n} value={n}>
                {n}
              </option>
            ))}
          </select>
        </label>
      ))}

      <label>
        Seed
        <input
          type="number"
          value={config.seed}
          onChange={(e) => reset({ seed: Number(e.target.value) })}
        />
      </label>
      <label title="Seeds the opening position — any number draws a different legal map">
        Map
        <input
          type="number"
          min={0}
          value={config.setupIndex}
          onChange={(e) => reset({ setupIndex: Number(e.target.value) })}
        />
      </label>
      <label>
        Bot delay
        <input
          type="number"
          min={0}
          max={2000}
          step={50}
          value={config.botDelay}
          onChange={(e) => reset({ botDelay: Number(e.target.value) })}
        />
      </label>
      <button className="primary" onClick={() => reset({ seed: config.seed + 1 })}>
        New game
      </button>
    </div>
  );
}
