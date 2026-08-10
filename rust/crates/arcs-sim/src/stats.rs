//! Reading a batch. Ported from `src/sim/stats.ts`.
//!
//! [`paired_stats`] is the one that earns strength claims; [`compute_stats`]
//! is the descriptive table the CLI prints.

use std::collections::BTreeMap;

use arcs_engine::{AmbitionId, BuildingKind, Player, ambition_count};

use crate::runner::SimResult;

#[derive(Clone, PartialEq, Debug)]
pub struct AgentStats {
    pub name: String,
    pub games: usize,
    pub wins: usize,
    pub win_rate: f64,
    /// 95% Wald interval on the win rate.
    pub win_rate_ci: f64,
    pub mean_power: f64,
    pub std_power: f64,
    pub mean_rank: f64,
    /// Mean count held at game end, per ambition (indexed by
    /// [`AmbitionId::as_index`]).
    pub mean_ambition: [f64; AmbitionId::COUNT],
    pub mean_cities: f64,
    pub mean_ships: f64,
    pub mean_guild_cards: f64,
}

#[derive(Clone, PartialEq, Debug)]
pub struct BatchStats {
    pub games: usize,
    pub players: usize,
    pub agents: Vec<AgentStats>,
    pub mean_chapters: f64,
    pub mean_rounds: f64,
    pub mean_battles: f64,
    pub mean_declared: f64,
    /// Power histogram in buckets of 5.
    pub histogram: Vec<HistogramBucket>,
    pub ms_per_game: f64,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct HistogramBucket {
    pub start: u8,
    pub count: usize,
}

/// A head-to-head comparison of two agents using the paired blocks.
///
/// The unit of observation is a *block* — one deal played from every seating —
/// not a game. Within a block both agents met identical cards, identical setup
/// and identical dice, so the difference in their block scores has the deal
/// divided out of it. The interval is then the standard error of those
/// differences.
///
/// FINDINGS is careful about what this does and does not buy, and the port
/// should be too: the spread is essentially unchanged against an unpaired
/// estimator (the trajectories diverge within a few decisions in a game this
/// branchy). **The win is removing the confound**, not narrowing the interval.
/// The answer to "how do I resolve a 5-point difference" is still "play more
/// games".
#[derive(Clone, PartialEq, Debug)]
pub struct PairedComparison {
    pub a: String,
    pub b: String,
    pub blocks: usize,
    /// Mean per-block win-share difference, in points. Positive favours `a`.
    pub diff: f64,
    /// 95% interval on `diff`, from the spread of block differences.
    pub ci: f64,
    /// Blocks where `a` scored more / fewer / the same wins than `b`.
    pub a_better: usize,
    pub b_better: usize,
    pub tied: usize,
    /// True when the interval excludes zero.
    pub separated: bool,
}

/// Compare agents `a` and `b` over the paired blocks. `None` when the batch
/// was not paired — an unpaired batch has no block to difference within, and
/// silently falling back to it is how the 14-point self-play bias survived.
pub fn paired_stats(
    sim: &SimResult,
    names: &[&str],
    a: usize,
    b: usize,
) -> Option<PairedComparison> {
    if !sim.paired || names.len() < 2 {
        return None;
    }

    // Win share per agent per block, so a block contributes one number each.
    // Keyed in block order rather than in arrival order: the parallel runner
    // reassembles blocks by index, and an order-dependent float sum would make
    // "byte-identical to the serial run" depend on the scheduler.
    let mut per_block: BTreeMap<usize, (usize, usize, usize)> = BTreeMap::new();
    for game in &sim.games {
        let row = per_block.entry(game.block).or_insert((0, 0, 0));
        row.2 += 1;
        let winner_agent = game.seating[game.result.winner];
        if winner_agent == a {
            row.0 += 1;
        } else if winner_agent == b {
            row.1 += 1;
        }
    }

    let mut diffs = Vec::with_capacity(per_block.len());
    let (mut a_better, mut b_better, mut tied) = (0usize, 0usize, 0usize);
    for (wins_a, wins_b, games) in per_block.into_values() {
        let d = (wins_a as f64 - wins_b as f64) / games as f64;
        diffs.push(d);
        if d > 0.0 {
            a_better += 1;
        } else if d < 0.0 {
            b_better += 1;
        } else {
            tied += 1;
        }
    }
    if diffs.is_empty() {
        return None;
    }

    let mean = diffs.iter().sum::<f64>() / diffs.len() as f64;
    let variance = if diffs.len() > 1 {
        diffs.iter().map(|d| (d - mean).powi(2)).sum::<f64>() / (diffs.len() - 1) as f64
    } else {
        0.0
    };
    let ci = 1.96 * (variance / diffs.len() as f64).sqrt();

    Some(PairedComparison {
        a: names[a].to_string(),
        b: names[b].to_string(),
        blocks: diffs.len(),
        diff: mean * 100.0,
        ci: ci * 100.0,
        a_better,
        b_better,
        tied,
        separated: mean.abs() > ci,
    })
}

/// The descriptive batch table: win rates, Power, ambitions, board counts.
pub fn compute_stats(sim: &SimResult, names: &[&str], ms_per_game: f64) -> BatchStats {
    struct Acc {
        games: usize,
        wins: usize,
        powers: Vec<f64>,
        ranks: u64,
        ambition: [u64; AmbitionId::COUNT],
        cities: u64,
        ships: u64,
        guild: u64,
    }

    let mut acc: Vec<Acc> = names
        .iter()
        .map(|_| Acc {
            games: 0,
            wins: 0,
            powers: Vec::new(),
            ranks: 0,
            ambition: [0; AmbitionId::COUNT],
            cities: 0,
            ships: 0,
            guild: 0,
        })
        .collect();

    let (mut chapters, mut rounds, mut battles, mut declared) = (0u64, 0u64, 0u64, 0u64);
    let mut all_powers: Vec<u8> = Vec::new();

    for game in &sim.games {
        let r = &game.result;
        chapters += r.chapters as u64;
        rounds += r.state.stats.rounds as u64;
        battles += r.state.stats.battles as u64;
        declared += r.state.stats.ambitions_declared as u64;

        for (seat, &agent_index) in game.seating.iter().enumerate() {
            let a = &mut acc[agent_index];
            a.games += 1;
            if r.winner == seat {
                a.wins += 1;
            }
            a.powers.push(r.power[seat] as f64);
            a.ranks += r.ranks[seat] as u64;
            all_powers.push(r.power[seat]);

            let ps = r.state.player(Player(seat as u8));
            for amb in AmbitionId::ALL {
                a.ambition[amb.as_index()] += ambition_count(ps, amb) as u64;
            }
            a.guild += ps.guild_cards.len() as u64;
            for sys in &r.state.systems {
                a.ships += (sys.fresh[seat] + sys.damaged[seat]) as u64;
                a.cities += sys
                    .buildings
                    .iter()
                    .filter(|b| b.player().as_index() == seat && b.kind() == BuildingKind::City)
                    .count() as u64;
            }
        }
    }

    let agents: Vec<AgentStats> = acc
        .iter()
        .zip(names)
        .map(|(a, name)| {
            let n = a.powers.len().max(1) as f64;
            let mean = a.powers.iter().sum::<f64>() / n;
            let variance = a.powers.iter().map(|p| (p - mean).powi(2)).sum::<f64>() / n;
            let games = a.games.max(1) as f64;
            let p = a.wins as f64 / games;
            AgentStats {
                name: (*name).to_string(),
                games: a.games,
                wins: a.wins,
                win_rate: p,
                win_rate_ci: 1.96 * (p * (1.0 - p) / games).sqrt(),
                mean_power: mean,
                std_power: variance.sqrt(),
                mean_rank: a.ranks as f64 / games,
                mean_ambition: core::array::from_fn(|i| a.ambition[i] as f64 / games),
                mean_cities: a.cities as f64 / games,
                mean_ships: a.ships as f64 / games,
                mean_guild_cards: a.guild as f64 / games,
            }
        })
        .collect();

    const BUCKET: u8 = 5;
    let mut histogram = Vec::new();
    if let (Some(&lo), Some(&hi)) = (all_powers.iter().min(), all_powers.iter().max()) {
        let lo = lo / BUCKET * BUCKET;
        let hi = hi / BUCKET * BUCKET;
        let mut start = lo;
        loop {
            histogram.push(HistogramBucket { start, count: 0 });
            if start >= hi {
                break;
            }
            start += BUCKET;
        }
        for p in &all_powers {
            histogram[((p - lo) / BUCKET) as usize].count += 1;
        }
    }

    let games = sim.games.len();
    let denom = games.max(1) as f64;
    BatchStats {
        games,
        players: names.len(),
        agents,
        mean_chapters: chapters as f64 / denom,
        mean_rounds: rounds as f64 / denom,
        mean_battles: battles as f64 / denom,
        mean_declared: declared as f64 / denom,
        histogram,
        ms_per_game,
    }
}
