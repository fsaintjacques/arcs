//! One game, and a batch of them. Ported from `src/sim/runner.ts`.
//!
//! The two functions here are the whole harness: [`play_game`] plays a seeded
//! game, and [`simulate_with_agents`] plays a *batch* under the two variance
//! controls FINDINGS records as hard-won. Everything else in the crate reads
//! their output.

use std::fmt;

use arcs_agents::{Agent, AgentCtx, AgentOpts, UnknownAgent, make_agent};
use arcs_engine::{
    Action, GameState, Pending, Player, Rng, RuleError, SetupMode, SplitMix64, VariantDef,
    apply_action_mut, get_pending, legal_actions, make_variant, new_game, observe,
    resolve_chance_mut, standings,
};

/// A boxed agent. The lifetime is what lets the gauntlet seat a [`Timed`]
/// wrapper that borrows its counters from the caller's stack.
///
/// [`Timed`]: crate::Timed
pub type BoxAgent<'a> = Box<dyn Agent + 'a>;

/// How to build an agent, the Rust form of the TS `{name, opts}` spec.
///
/// The harness rebuilds agents from specs on every rayon worker rather than
/// sharing one table, exactly as the TS pool rebuilds them past a structured
/// clone. The convenient side effect is the same in both languages: anything
/// measurable has to be registered in the agent registry, so a bot cannot
/// enter the ledger under a configuration nobody else can reproduce.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct AgentSpec {
    pub name: String,
    pub opts: AgentOpts,
}

impl AgentSpec {
    pub fn new(name: impl Into<String>) -> Self {
        AgentSpec {
            name: name.into(),
            opts: AgentOpts::default(),
        }
    }

    pub fn with_opts(name: impl Into<String>, opts: AgentOpts) -> Self {
        AgentSpec {
            name: name.into(),
            opts,
        }
    }

    pub fn build(&self) -> Result<BoxAgent<'static>, SimError> {
        make_agent(&self.name, &self.opts).map_err(|_: UnknownAgent| SimError::Unknown {
            name: self.name.clone(),
        })
    }
}

/// Build one agent per spec, in seat order.
pub fn build_table(specs: &[AgentSpec]) -> Result<Vec<BoxAgent<'static>>, SimError> {
    specs.iter().map(AgentSpec::build).collect()
}

/// Everything that can stop a batch. The TS harness `throw`s for all of these;
/// the port makes them a value so a long run reports which game failed instead
/// of unwinding out of a rayon worker.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum SimError {
    /// The registry does not know this agent name.
    Unknown { name: String },
    /// The engine refused an action it had just offered — a rules bug, not a
    /// harness one.
    Rule { seed: u64, err: RuleError },
    /// An agent answered with an index outside the legal slice.
    OutOfRange {
        agent: String,
        index: usize,
        legal: usize,
    },
    /// The livelock guard fired. A game that will not end is worth a test
    /// failure rather than a hung batch — see the catapult loop in FINDINGS.
    NotTerminated { seed: u64, decisions: usize },
}

impl fmt::Display for SimError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SimError::Unknown { name } => write!(f, "unknown agent `{name}`"),
            SimError::Rule { seed, err } => write!(f, "seed {seed}: {err}"),
            SimError::OutOfRange {
                agent,
                index,
                legal,
            } => write!(f, "{agent} chose index {index} of {legal} legal actions"),
            SimError::NotTerminated { seed, decisions } => {
                write!(f, "seed {seed}: no result after {decisions} decisions")
            }
        }
    }
}

impl core::error::Error for SimError {}

/// One finished game.
///
/// Unlike the TS result this does not carry the [`VariantDef`]: a variant is
/// ~1 KB of resolved map and a batch holds every result at once, while the
/// variant is a pure function of `(players, setup_index, setup_mode)` — all
/// three of which are here.
#[derive(Clone, PartialEq, Debug)]
pub struct GameResult {
    pub seed: u64,
    pub setup_index: u64,
    pub setup_mode: SetupMode,
    pub state: GameState,
    /// Final Power per **seat**.
    pub power: Vec<u8>,
    /// Winning seat.
    pub winner: usize,
    /// Finishing rank per seat, 0 = winner.
    pub ranks: Vec<u8>,
    pub chapters: u8,
    pub decisions: usize,
    /// Decisions taken by each seat.
    pub decisions_by: Vec<u32>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct PlayOptions {
    pub players: u8,
    pub seed: u64,
    pub setup_index: u64,
    /// `Deck` (the game as played) draws one of the four printed setup cards
    /// for this player count; `Draw` invents a fresh legal opening per seed.
    pub setup_mode: SetupMode,
    /// Hard stop, to turn an engine livelock into a test failure.
    pub max_decisions: usize,
}

impl Default for PlayOptions {
    fn default() -> Self {
        PlayOptions {
            players: 3,
            seed: 1,
            setup_index: 0,
            setup_mode: SetupMode::Deck,
            max_decisions: 200_000,
        }
    }
}

/// Domain separation for the per-seat agent streams, so forking them neither
/// consumes nor shadows the game's own chance stream.
const SEAT_STREAM: u64 = 0x5EA7_0000_5EED;

/// Play one seeded game to completion. Agents are seated in slice order.
pub fn play_game(agents: &mut [BoxAgent<'_>], opts: &PlayOptions) -> Result<GameResult, SimError> {
    play_game_logged(agents, opts, &mut |_, _, _| {})
}

/// [`play_game`] with the TS runner's `onDecision` hook: called with the state
/// *before* the chosen action is applied. The CLI's turn log and the ported
/// "only ever plays an action that was on offer" test both read it.
pub fn play_game_logged(
    agents: &mut [BoxAgent<'_>],
    opts: &PlayOptions,
    on_decision: &mut dyn FnMut(&GameState, Player, Action),
) -> Result<GameResult, SimError> {
    let v = make_variant(opts.players, opts.setup_index, opts.setup_mode);
    let mut seated: Vec<&mut (dyn Agent + '_)> = agents.iter_mut().map(Box::as_mut).collect();
    play_game_in(&v, &mut seated, opts, on_decision)
}

/// [`play_game`] against an already-built variant and an already-seated table.
///
/// A whole paired block shares one deal and therefore one variant, and
/// resolving the map's adjacency is not free, so the batch runner builds it
/// once per block. The TS runner rebuilds it per game; nothing observable
/// changes, because a variant is a pure function of the arguments this asserts.
pub(crate) fn play_game_in<'a>(
    v: &VariantDef,
    agents: &mut [&mut (dyn Agent + 'a)],
    opts: &PlayOptions,
    on_decision: &mut dyn FnMut(&GameState, Player, Action),
) -> Result<GameResult, SimError> {
    debug_assert_eq!(v.players, opts.players, "variant is for another table size");
    debug_assert_eq!(agents.len(), opts.players as usize, "one agent per seat");

    // The game's own chance stream: the deal, the dice, every `resolve_chance`.
    let mut rng = SplitMix64::new(opts.seed);
    let mut s = new_game(v, &mut rng, opts.setup_index, opts.setup_mode);

    // Per-seat agent streams, forked from the game seed and from nothing else.
    //
    // TS derives them arithmetically (`mulberry32(seed ^ 0x9e3779b9*(p+1))`);
    // the port keeps the *property* rather than the arithmetic, per plan §5.
    // The property is what matters: each seat draws from its own stream, so a
    // search agent sampling ten thousand worlds cannot shift what the seat
    // beside it rolls. Without that, "same batch, one agent swapped" would not
    // be a controlled comparison — the swap would move every other seat's
    // randomness too, and every ablation in FINDINGS would be reading the RNG.
    let mut fork = SplitMix64::new(opts.seed ^ SEAT_STREAM);
    let mut ctxs: Vec<AgentCtx> = (0..opts.players)
        .map(|p| AgentCtx::new(v, Player(p), fork.fork_seed()))
        .collect();

    let mut legal: Vec<Action> = Vec::new();
    let mut decisions = 0usize;
    let mut decisions_by = vec![0u32; opts.players as usize];

    loop {
        match get_pending(&s, v) {
            Pending::Over => break,
            Pending::Chance => {
                resolve_chance_mut(&mut s, v, &mut rng).map_err(|err| SimError::Rule {
                    seed: opts.seed,
                    err,
                })?
            }
            Pending::Decision { player } => {
                decisions += 1;
                if decisions > opts.max_decisions {
                    return Err(SimError::NotTerminated {
                        seed: opts.seed,
                        decisions,
                    });
                }
                legal_actions(&s, v, &mut legal);
                let obs = observe(&s, v, player);
                let seat = player.as_index();
                let i = agents[seat].choose(&obs, &legal, &mut ctxs[seat]);
                if i >= legal.len() {
                    return Err(SimError::OutOfRange {
                        agent: agents[seat].name().to_string(),
                        index: i,
                        legal: legal.len(),
                    });
                }
                on_decision(&s, player, legal[i]);
                decisions_by[seat] += 1;
                apply_action_mut(&mut s, v, legal[i]).map_err(|err| SimError::Rule {
                    seed: opts.seed,
                    err,
                })?;
            }
        }
    }

    let table = standings(&s);
    let mut ranks = vec![0u8; opts.players as usize];
    for row in table.iter() {
        ranks[row.player.as_index()] = row.rank;
    }

    Ok(GameResult {
        seed: opts.seed,
        setup_index: opts.setup_index,
        setup_mode: opts.setup_mode,
        power: (0..opts.players)
            .map(|p| s.player(Player(p)).power)
            .collect(),
        winner: table.as_slice()[0].player.as_index(),
        ranks,
        chapters: s.chapter,
        decisions,
        decisions_by,
        state: s,
    })
}

/// Every permutation of `n` seats, in a stable order. `n <= 4`, so at most 24.
///
/// (`permutations` in runner.ts — the same insertion order, so a Rust batch
/// visits seatings in the same sequence a TS one does.)
pub fn permutations(n: usize) -> Vec<Vec<usize>> {
    if n <= 1 {
        return vec![vec![0]];
    }
    let mut out = Vec::new();
    for rest in permutations(n - 1) {
        for i in 0..=rest.len() {
            let mut p = Vec::with_capacity(rest.len() + 1);
            p.extend_from_slice(&rest[..i]);
            p.push(n - 1);
            p.extend_from_slice(&rest[i..]);
            out.push(p);
        }
    }
    out
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct SimOptions {
    pub players: u8,
    pub games: usize,
    pub seed: u64,
    pub setup_index: u64,
    pub setup_mode: SetupMode,
    pub max_decisions: usize,
    /// Cycle agents through every seating permutation (default true).
    pub rotate_seats: bool,
    /// Hold the deal fixed across each block of `n!` seatings (default true),
    /// so every agent meets the identical game in every seat.
    pub paired: bool,
    /// Run only these block indices instead of `0..block_count`.
    ///
    /// A block's games depend on nothing but its index, so a batch can be
    /// partitioned across threads and reassembled without changing a single
    /// seed or seating — [`simulate_parallel`] leans on exactly this, and the
    /// byte-identical guarantee it advertises is a restatement of it.
    ///
    /// [`simulate_parallel`]: crate::simulate_parallel
    pub blocks: Option<Vec<usize>>,
}

impl Default for SimOptions {
    fn default() -> Self {
        SimOptions {
            players: 3,
            games: 100,
            seed: 1,
            setup_index: 0,
            setup_mode: SetupMode::Deck,
            max_decisions: 200_000,
            rotate_seats: true,
            paired: true,
            blocks: None,
        }
    }
}

impl SimOptions {
    /// Games per block: `n!` when paired, otherwise 1.
    pub fn block_size(&self) -> usize {
        if self.rotate_seats && self.paired {
            permutations(self.players as usize).len()
        } else {
            1
        }
    }

    /// Blocks a full batch covers. `games` is rounded **up** to whole blocks:
    /// a half-finished block would reintroduce exactly the seat bias
    /// permuting is there to remove.
    pub fn block_count(&self) -> usize {
        self.games.div_ceil(self.block_size())
    }
}

/// One played game plus the seating it was played under. `seating[seat]` is
/// the index of the agent that sat there.
#[derive(Clone, PartialEq, Debug)]
pub struct SeatedGame {
    pub result: GameResult,
    pub seating: Vec<usize>,
    pub block: usize,
}

#[derive(Clone, PartialEq, Debug)]
pub struct SimResult {
    /// Results in play order, plus the seating used for each.
    pub games: Vec<SeatedGame>,
    /// Games per block: `n!` when paired, otherwise 1.
    pub block_size: usize,
    pub paired: bool,
}

/// A block's deal seed. Derived from **nothing but** the batch seed and the
/// block index, which is the property the parallel runner is built on: a
/// worker handed block 37 reproduces block 37, wherever it runs and whatever
/// its neighbours are doing.
fn block_seed(base: u64, block: usize) -> u64 {
    SplitMix64::new(base ^ (block as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)).next_u64()
}

/// Run a batch from agent specs. See [`simulate_with_agents`] for the method.
pub fn simulate(specs: &[AgentSpec], opts: &SimOptions) -> Result<SimResult, SimError> {
    let mut agents = build_table(specs)?;
    simulate_with_agents(&mut agents, opts)
}

/// Run a batch. `seating[seat] = agent index`.
///
/// Two variance controls, and they do different jobs:
///
/// **Permuted seating.** Seating cycles through every *permutation*, not just
/// rotations. Rotating alone leaves the agents' cyclic order fixed, and in a
/// lead-and-follow game that is a real advantage — sitting immediately after a
/// weak player is worth several points a game. FINDINGS measured it: two
/// identical greedy agents posted 78.3% / 21.7% under rotation and 50.0% /
/// 50.0% under permutation.
///
/// **Common random numbers.** The deal is held fixed across each block of `n!`
/// seatings, so within a block every agent plays the *same game* from every
/// seat. Without this the permutations are spread across `n!` unrelated deals
/// and cancel nothing: the agent that happened to draw the better cards wins,
/// and the comparison measures the shuffle. This is the fix for the worst
/// measurement bug in the project — the harness beating an agent with a copy
/// of itself by 14 points — and [`paired_stats`] reads the within-block
/// differences it creates.
///
/// [`paired_stats`]: crate::paired_stats
pub fn simulate_with_agents(
    agents: &mut [BoxAgent<'_>],
    opts: &SimOptions,
) -> Result<SimResult, SimError> {
    let n = agents.len();
    let perms = permutations(n);
    let block_size = opts.block_size();
    let identity: Vec<usize> = (0..n).collect();

    let default_blocks: Vec<usize>;
    let blocks: &[usize] = match &opts.blocks {
        Some(list) => list,
        None => {
            default_blocks = (0..opts.block_count()).collect();
            &default_blocks
        }
    };

    let mut games = Vec::with_capacity(blocks.len() * block_size);
    for &block in blocks {
        // One deal per block when paired; blocks are single games otherwise,
        // so the block index is the deal index either way.
        let seed = block_seed(opts.seed, block);
        let setup_index = opts.setup_index + block as u64;
        let play = PlayOptions {
            players: opts.players,
            seed,
            setup_index,
            setup_mode: opts.setup_mode,
            max_decisions: opts.max_decisions,
        };
        let v = make_variant(opts.players, setup_index, opts.setup_mode);

        for k in 0..block_size {
            let i = block * block_size + k;
            let seating: &[usize] = if opts.rotate_seats {
                let pick = if opts.paired { k } else { i };
                &perms[pick % perms.len()]
            } else {
                &identity
            };
            // Seat the agents by permuting *borrows*: each agent keeps one
            // identity across the whole batch, so its scratch buffers and
            // search arena are reused rather than reallocated per game.
            let mut pool: Vec<Option<&mut (dyn Agent + '_)>> =
                agents.iter_mut().map(|a| Some(Box::as_mut(a))).collect();
            let mut seated: Vec<&mut (dyn Agent + '_)> = seating
                .iter()
                .map(|&agent_index| {
                    pool[agent_index]
                        .take()
                        .expect("a seating must be a permutation")
                })
                .collect();
            let result = play_game_in(&v, &mut seated, &play, &mut |_, _, _| {})?;
            games.push(SeatedGame {
                result,
                seating: seating.to_vec(),
                block,
            });
        }
    }

    Ok(SimResult {
        games,
        block_size,
        paired: opts.rotate_seats && opts.paired,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Ports the `permutations` contract runner.ts states: `n!` distinct
    /// orderings, each a genuine permutation.
    #[test]
    fn permutations_are_complete_and_distinct() {
        for n in 1..=4 {
            let perms = permutations(n);
            let factorial: usize = (1..=n).product();
            assert_eq!(perms.len(), factorial);
            for p in &perms {
                let mut sorted = p.clone();
                sorted.sort_unstable();
                assert_eq!(sorted, (0..n).collect::<Vec<_>>());
            }
            let mut unique = perms.clone();
            unique.sort();
            unique.dedup();
            assert_eq!(unique.len(), factorial);
        }
    }

    #[test]
    fn a_block_seed_depends_on_nothing_but_the_batch_seed_and_block_index() {
        assert_eq!(block_seed(7, 3), block_seed(7, 3));
        assert_ne!(block_seed(7, 3), block_seed(7, 4));
        assert_ne!(block_seed(7, 3), block_seed(8, 3));
    }

    #[test]
    fn games_round_up_to_whole_blocks() {
        let opts = SimOptions {
            players: 3,
            games: 7,
            ..SimOptions::default()
        };
        assert_eq!(opts.block_size(), 6);
        assert_eq!(opts.block_count(), 2);
    }
}
