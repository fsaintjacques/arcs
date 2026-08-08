import { AMBITIONS, ambitionCount, type AmbitionId } from '../engine';
import type { SimResult } from './runner';

export interface AgentStats {
  name: string;
  games: number;
  wins: number;
  winRate: number;
  /** 95% Wald interval on the win rate. */
  winRateCi: number;
  meanPower: number;
  stdPower: number;
  meanRank: number;
  /** Mean count held at game end, per ambition. */
  meanAmbition: Record<AmbitionId, number>;
  meanCities: number;
  meanShips: number;
  meanGuildCards: number;
}

export interface BatchStats {
  games: number;
  players: number;
  agents: AgentStats[];
  meanChapters: number;
  meanRounds: number;
  meanBattles: number;
  meanDeclared: number;
  /** Power histogram in buckets of 5. */
  histogram: { start: number; count: number }[];
  msPerGame: number;
}

export function computeStats(
  sim: SimResult,
  names: string[],
  msPerGame: number,
): BatchStats {
  const n = names.length;
  const acc = names.map((name) => ({
    name,
    games: 0,
    wins: 0,
    powers: [] as number[],
    ranks: 0,
    ambition: Object.fromEntries(AMBITIONS.map((a) => [a, 0])) as Record<AmbitionId, number>,
    cities: 0,
    ships: 0,
    guild: 0,
  }));

  let chapters = 0;
  let rounds = 0;
  let battles = 0;
  let declared = 0;
  const allPowers: number[] = [];

  for (const { result, seating } of sim.games) {
    chapters += result.chapters;
    rounds += result.state.stats.rounds;
    battles += result.state.stats.battles;
    declared += result.state.stats.ambitionsDeclared;

    seating.forEach((agentIndex, seat) => {
      const a = acc[agentIndex];
      a.games++;
      if (result.winner === seat) a.wins++;
      a.powers.push(result.power[seat]);
      a.ranks += result.ranks[seat];
      allPowers.push(result.power[seat]);

      const ps = result.state.playerStates[seat];
      for (const amb of AMBITIONS) a.ambition[amb] += ambitionCount(ps, amb);
      a.guild += ps.guildCards.length;
      for (const sys of result.state.systems) {
        a.ships += sys.fresh[seat] + sys.damaged[seat];
        a.cities += sys.buildings.filter((b) => b.player === seat && b.kind === 'city').length;
      }
    });
  }

  const agents: AgentStats[] = acc.map((a) => {
    const mean = a.powers.reduce((x, y) => x + y, 0) / Math.max(1, a.powers.length);
    const variance =
      a.powers.reduce((x, y) => x + (y - mean) ** 2, 0) / Math.max(1, a.powers.length);
    const p = a.wins / Math.max(1, a.games);
    return {
      name: a.name,
      games: a.games,
      wins: a.wins,
      winRate: p,
      winRateCi: 1.96 * Math.sqrt((p * (1 - p)) / Math.max(1, a.games)),
      meanPower: mean,
      stdPower: Math.sqrt(variance),
      meanRank: a.ranks / Math.max(1, a.games),
      meanAmbition: Object.fromEntries(
        AMBITIONS.map((amb) => [amb, a.ambition[amb] / Math.max(1, a.games)]),
      ) as Record<AmbitionId, number>,
      meanCities: a.cities / Math.max(1, a.games),
      meanShips: a.ships / Math.max(1, a.games),
      meanGuildCards: a.guild / Math.max(1, a.games),
    };
  });

  const bucket = 5;
  const histogram: { start: number; count: number }[] = [];
  if (allPowers.length > 0) {
    const lo = Math.floor(Math.min(...allPowers) / bucket) * bucket;
    const hi = Math.floor(Math.max(...allPowers) / bucket) * bucket;
    for (let b = lo; b <= hi; b += bucket) histogram.push({ start: b, count: 0 });
    for (const p of allPowers) histogram[Math.floor((p - lo) / bucket)].count++;
  }

  const games = sim.games.length;
  return {
    games,
    players: n,
    agents,
    meanChapters: chapters / Math.max(1, games),
    meanRounds: rounds / Math.max(1, games),
    meanBattles: battles / Math.max(1, games),
    meanDeclared: declared / Math.max(1, games),
    histogram,
    msPerGame,
  };
}
