/** Run bot-vs-bot batches in a worker and show the ladder. */
import { useEffect, useRef, useState } from 'react';
import { agentNames } from '../../agents';
import type { BatchStats, PairedComparison } from '../../sim/stats';
import type { SimMessage, SimRequest } from '../simWorker';
import { AMBITIONS } from '../../engine/types';
import { AMBITION_LABEL } from '../describe';

export function SimPanel() {
  const [seats, setSeats] = useState<string[]>(['greedy', 'greedy', 'random']);
  const [games, setGames] = useState(60);
  const [seed, setSeed] = useState(1);
  const [progress, setProgress] = useState<{ done: number; total: number } | null>(null);
  const [stats, setStats] = useState<BatchStats | null>(null);
  const [paired, setPaired] = useState<PairedComparison | null>(null);
  const [error, setError] = useState<string | null>(null);
  const worker = useRef<Worker | null>(null);

  useEffect(() => () => worker.current?.terminate(), []);

  const run = () => {
    worker.current?.terminate();
    setStats(null);
    setPaired(null);
    setError(null);
    setProgress({ done: 0, total: games });

    const w = new Worker(new URL('../simWorker.ts', import.meta.url), { type: 'module' });
    worker.current = w;
    w.onmessage = (e: MessageEvent<SimMessage>) => {
      if (e.data.kind === 'progress') setProgress({ done: e.data.done, total: e.data.total });
      else if (e.data.kind === 'done') {
        setStats(e.data.stats);
        setPaired(e.data.paired);
        setProgress(null);
      } else {
        setError(e.data.message);
        setProgress(null);
      }
    };
    w.postMessage({ agents: seats, games, seed, setupIndex: 0 } satisfies SimRequest);
  };

  const setSeat = (i: number, name: string) => {
    setSeats((s) => s.map((v, j) => (i === j ? name : v)));
  };

  return (
    <div className="sim">
      <section className="panel">
        <h2>Batch</h2>
        <div className="sim-controls">
          {seats.map((name, i) => (
            <label key={i}>
              Seat {i}
              <select value={name} onChange={(e) => setSeat(i, e.target.value)}>
                {agentNames().map((n) => (
                  <option key={n} value={n}>
                    {n}
                  </option>
                ))}
              </select>
            </label>
          ))}
          <label>
            Players
            <select
              value={seats.length}
              onChange={(e) => {
                const n = Number(e.target.value);
                setSeats((s) =>
                  Array.from({ length: n }, (_, i) => s[i] ?? 'greedy'),
                );
              }}
            >
              {[2, 3, 4].map((n) => (
                <option key={n} value={n}>
                  {n}
                </option>
              ))}
            </select>
          </label>
          <label>
            Games
            <input
              type="number"
              min={1}
              max={2000}
              value={games}
              onChange={(e) => setGames(Number(e.target.value))}
            />
          </label>
          <label>
            Seed
            <input type="number" value={seed} onChange={(e) => setSeed(Number(e.target.value))} />
          </label>
          <button className="primary" onClick={run} disabled={progress !== null}>
            {progress ? 'Running…' : 'Run batch'}
          </button>
        </div>
        {progress && (
          <div className="progress">
            <div style={{ width: `${(100 * progress.done) / progress.total}%` }} />
            <span>
              {progress.done} / {progress.total}
            </span>
          </div>
        )}
        {error && <p className="warn">{error}</p>}
        <p className="hint">
          Each deal is played from every seating, so identical agents score identically and the
          same cards reach both — any gap is strength, not seat luck or the shuffle.
        </p>
      </section>

      {paired && (
        <section className="panel">
          <h2>
            Head-to-head — {paired.a} vs {paired.b}
          </h2>
          <p>
            <strong>
              {paired.diff >= 0 ? '+' : ''}
              {paired.diff.toFixed(1)} ±{paired.ci.toFixed(1)}
            </strong>{' '}
            points of win share to {paired.a}, over {paired.blocks} paired deals.
          </p>
          <p className={paired.separated ? 'hint' : 'warn'}>
            {paired.separated
              ? 'The interval excludes zero — a real difference at this sample size.'
              : 'The interval covers zero — these agents are NOT separated at this sample size.'}
          </p>
          <p className="dim">
            Deals won by {paired.a}: {paired.aBetter} · by {paired.b}: {paired.bBetter} · split:{' '}
            {paired.tied}
          </p>
        </section>
      )}

      {stats && (
        <>
          <section className="panel">
            <h2>Results — {stats.games} games</h2>
            <p className="dim">
              {stats.meanChapters.toFixed(2)} chapters · {stats.meanRounds.toFixed(1)} rounds ·{' '}
              {stats.meanBattles.toFixed(1)} battles · {stats.meanDeclared.toFixed(1)} ambitions
              declared · {stats.msPerGame.toFixed(0)} ms/game
            </p>
            <table className="results">
              <thead>
                <tr>
                  <th>Agent</th>
                  <th>Win %</th>
                  <th>Power</th>
                  <th>Rank</th>
                  <th>Cities</th>
                  <th>Guild</th>
                </tr>
              </thead>
              <tbody>
                {stats.agents.map((a, i) => (
                  <tr key={i}>
                    <td>{a.name}</td>
                    <td className="num">
                      {(a.winRate * 100).toFixed(1)}
                      <span className="dim"> ±{(a.winRateCi * 100).toFixed(1)}</span>
                    </td>
                    <td className="num">
                      {a.meanPower.toFixed(1)}
                      <span className="dim"> ±{a.stdPower.toFixed(1)}</span>
                    </td>
                    <td className="num">{a.meanRank.toFixed(2)}</td>
                    <td className="num">{a.meanCities.toFixed(1)}</td>
                    <td className="num">{a.meanGuildCards.toFixed(1)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </section>

          <section className="panel">
            <h2>Ambition counts at game end</h2>
            <table className="results">
              <thead>
                <tr>
                  <th>Agent</th>
                  {AMBITIONS.map((a) => (
                    <th key={a}>{AMBITION_LABEL[a]}</th>
                  ))}
                </tr>
              </thead>
              <tbody>
                {stats.agents.map((a, i) => (
                  <tr key={i}>
                    <td>{a.name}</td>
                    {AMBITIONS.map((amb) => (
                      <td key={amb} className="num">
                        {a.meanAmbition[amb].toFixed(1)}
                      </td>
                    ))}
                  </tr>
                ))}
              </tbody>
            </table>
          </section>

          <section className="panel">
            <h2>Final Power, all seats</h2>
            <div className="histogram">
              {stats.histogram.map((h) => {
                const max = Math.max(...stats.histogram.map((x) => x.count), 1);
                return (
                  <div key={h.start} className="bar-row">
                    <span className="bar-label">
                      {h.start}–{h.start + 4}
                    </span>
                    <div className="bar" style={{ width: `${(100 * h.count) / max}%` }} />
                    <span className="bar-count">{h.count}</span>
                  </div>
                );
              })}
            </div>
          </section>
        </>
      )}
    </div>
  );
}
