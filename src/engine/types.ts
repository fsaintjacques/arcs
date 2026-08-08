/**
 * Core types for the Arcs engine.
 *
 * Arcs is modelled as an explicit sequential decision process with imperfect
 * information:
 *
 *   - **decision nodes** name the player to move and enumerate every legal
 *     `Action`, so any algorithm can drive the game;
 *   - **chance nodes** (dealing action cards, rolling battle dice, drawing
 *     Court cards) are resolved by an injectable seeded RNG, so games replay
 *     exactly and search algorithms can sample;
 *   - hidden state (hands, deck order) is separated by `observe()` so agents
 *     can be given a legal view and re-sample worlds from it (`determinize()`).
 *
 * Everything an agent needs is reachable from `GameState` + `VariantDef`;
 * nothing in here depends on the UI or the simulator.
 */

// ---------------------------------------------------------------------------
// Vocabulary
// ---------------------------------------------------------------------------

/** The 5 resource types, which are also the 5 planet types and Guild suits. */
export type ResourceType = 'material' | 'fuel' | 'weapon' | 'relic' | 'psionic';

export const RESOURCE_TYPES: ResourceType[] = [
  'material',
  'fuel',
  'weapon',
  'relic',
  'psionic',
];

/** The 4 action-card suits. */
export type Suit = 'administration' | 'aggression' | 'construction' | 'mobilization';

export const SUITS: Suit[] = ['administration', 'aggression', 'construction', 'mobilization'];

/** The 7 standard actions an action pip can buy. */
export type ActionKind = 'tax' | 'build' | 'move' | 'repair' | 'influence' | 'secure' | 'battle';

export type AmbitionId = 'tycoon' | 'tyrant' | 'warlord' | 'keeper' | 'empath';

export const AMBITIONS: AmbitionId[] = ['tycoon', 'tyrant', 'warlord', 'keeper', 'empath'];

export type DieType = 'assault' | 'skirmish' | 'raid';

export type BuildingKind = 'city' | 'starport';

/** How a follower played into the trick. `lead` is the initiative holder. */
export type PlayMode = 'lead' | 'surpass' | 'copy' | 'pivot';

// ---------------------------------------------------------------------------
// Static definitions (the "variant" — see map.ts / cards.ts / court.ts)
// ---------------------------------------------------------------------------

/** A system is a gate or a planet. Gates carry their cluster's number. */
export interface SystemDef {
  id: number;
  cluster: number;
  /** 0 for the gate, 1..3 for the cluster's planets. */
  slot: number;
  kind: 'gate' | 'planet';
  /** Planets only: the resource type taxed from cities here. */
  planetType: ResourceType | null;
  /** Planets only: how many buildings fit here (1 or 2). */
  buildingSlots: number;
  /** Base adjacency, ignoring out-of-play clusters and path markers. */
  adjacent: number[];
  label: string;
}

export interface ActionCardDef {
  id: number;
  suit: Suit;
  /** 1..7. With 2-3 players only 2..6 are in the deck. */
  number: number;
  /** Actions granted when led or surpassed with. */
  pips: number;
  /** Ambition this card can declare when led; null for "1", 'any' for "7". */
  ambition: AmbitionId | 'any' | null;
}

export interface CourtCardDef {
  id: number;
  /** The number printed on the card: 01-25 Guild, 26-31 Vox. */
  number: number;
  name: string;
  kind: 'guild' | 'vox';
  /** Guild cards only: counts toward ambitions exactly like a resource token. */
  suit: ResourceType | null;
  /** Keys an attacker must spend to steal this card in a raid. */
  raidCost: number;
  /** The card's printed rules text, verbatim. */
  text: string;
  /**
   * Engine-readable form of a Guild card's ability. `undefined` means the card
   * has no printed ability; see `UNIMPLEMENTED_POWERS` in court.ts for the ones
   * the engine does not yet act on.
   */
  power?: import('./court').CardPower;
  /** Vox cards only: what resolves when the card is secured. */
  vox?: import('./court').VoxEffect;
  /** Vox cards resolve and are discarded rather than kept. */
  discardOnSecure?: boolean;
}

/** One face of a battle die. */
export interface DieFace {
  /** Hits the attacker must take on their own attacking ships. */
  selfHits: number;
  /** Triggers the defender's interception (resolved once per battle). */
  intercept: number;
  /** Hits on defending ships, spilling onto buildings once ships are gone. */
  hits: number;
  /** Hits that only ever land on defending buildings. */
  buildingHits: number;
  /** Keys spent to steal resources and Guild cards. */
  keys: number;
}

/** An ambition marker: Power for first and second place, per side. */
export interface AmbitionMarkerDef {
  blue: { first: number; second: number };
  orange: { first: number; second: number };
}

/** A game variant = player count + map + decks + thresholds. */
export interface VariantDef {
  id: string;
  name: string;
  players: number;
  systems: SystemDef[];
  actionDeck: ActionCardDef[];
  courtDeck: CourtCardDef[];
  ambitionMarkers: AmbitionMarkerDef[];
  /** Face-up Court row width: 3 at 2 players, 4 at 3-4. */
  courtRowSize: number;
  /** Power that ends the game at a chapter break. */
  powerThreshold: number;
  maxChapters: number;
  handSize: number;
}

// ---------------------------------------------------------------------------
// Mutable game state
// ---------------------------------------------------------------------------

export type RNG = () => number;

export interface Building {
  player: number;
  kind: BuildingKind;
  damaged: boolean;
  /** Cities can only be taxed once per turn. */
  taxedThisTurn: boolean;
  /** Starports can only build one ship per turn. */
  builtThisTurn: boolean;
}

export interface SystemState {
  /** Fresh ship count per player. */
  fresh: number[];
  /** Damaged ship count per player. */
  damaged: number[];
  buildings: Building[];
  outOfPlay: boolean;
}

export interface PlayerState {
  power: number;
  /**
   * Resource slots. `null` is an open but empty slot; slots beyond
   * `openResourceSlots` are covered by unbuilt cities and unusable.
   */
  resources: (ResourceType | null)[];
  /** Outrage marker per resource type: cannot be spent for its Prelude action. */
  outrage: Record<ResourceType, boolean>;
  /** Court card ids held (Guild cards only — Vox cards discard on secure). */
  guildCards: number[];
  /**
   * Destroyed Rival pieces: counts toward Warlord. Owner and kind are kept so
   * the pieces can go home when Warlord scores and Trophies are returned (p19).
   */
  trophies: { owner: number; kind: 'ship' | 'starport' | 'city' }[];
  /** Captured Rival agents, by owner: counts toward Tyrant. */
  captives: number[];
  agentsSupply: number;
  shipsSupply: number;
  starportsSupply: number;
  /** Cities placed on the map or destroyed — how far the board is uncovered. */
  citiesUsed: number;
  hand: number[];
}

export interface CourtSlot {
  card: number;
  /** Agents on this card, per player. */
  agents: number[];
}

/** One card played into the current round's trick. */
export interface PlayedCard {
  player: number;
  card: number;
  mode: PlayMode;
  /** Copy plays and seize cards are face down — hidden from other players. */
  faceDown: boolean;
}

/** The state of the round in progress. */
export interface RoundState {
  /** Index into `turnOrder` of the player currently taking a turn. */
  turnIndex: number;
  /** Player order for this round, starting from the initiative holder. */
  turnOrder: number[];
  lead: PlayedCard | null;
  /** Lead card's effective number: 0 once an ambition is declared on it. */
  leadNumber: number;
  played: PlayedCard[];
  /** Player who seized the initiative this round, if any. */
  seizedBy: number | null;
  /** Consecutive passes, to detect "everyone with cards passed" (p8). */
  consecutivePasses: number;
}

/** The turn in progress: what the current player has left to spend. */
export interface TurnState {
  player: number;
  mode: PlayMode;
  /** Actions still available from the played card's pips. */
  pipsLeft: number;
  /** Which action kinds the remaining pips may buy. */
  pipActions: ActionKind[];
  /** Free actions granted by spent resources, resolved one at a time. */
  freeActions: ActionKind[][];
  /** A Weapon spent in the Prelude adds Battle to this turn's pip actions. */
  weaponSpent: boolean;
  /** Prelude is over once the first pip is spent or actions have begun. */
  preludeOver: boolean;
  /** Ambition already declared this turn — cannot declare twice on one card. */
  declaredThisTurn: boolean;
  /** Resources spent in the Prelude, returned to the supply when it ends. */
  preludeSpent: ResourceType[];
  /** Court cards secured this turn: their Preludes are unusable (p20). */
  securedThisPrelude: number[];
}

/** A battle mid-resolution. */
export interface BattleState {
  system: number;
  attacker: number;
  defender: number;
  dice: Record<DieType, number>;
  /** Rolled totals, consumed as they are assigned. */
  selfHits: number;
  intercept: number;
  hits: number;
  buildingHits: number;
  keys: number;
  /** Set once the intercept has been converted into self-hits. */
  interceptResolved: boolean;
}

/** A catapult move in progress (p13). */
export interface MoveState {
  /** Ships still travelling, and where they are. */
  at: number;
  ships: number;
  /**
   * Systems this catapult has already passed through. Revisiting one can only
   * undo progress, and allowing it makes the move non-terminating, so the
   * engine forbids it (see the ruling in game.ts).
   */
  visited: number[];
}

export type Phase =
  | 'deal' // chance: shuffle and deal a chapter's hands
  | 'mulligan' // decision: 2-player mulligan
  | 'play' // decision: play a card (lead / surpass / copy / pivot) or pass
  | 'prelude' // decision: declare, seize, spend resources, then begin actions
  | 'actions' // decision: spend pips and free actions, or end the turn
  | 'catapult' // decision: continue or stop a catapult move
  | 'battleRoll' // chance: roll the collected battle dice
  | 'battleAssign' // decision: assign hits one at a time, then raid
  | 'reinforce' // decision: a wiped-out player places 3 ships in a gate
  | 'over';

export interface GameStats {
  rounds: number;
  chapters: number;
  battles: number;
  cardsPlayed: number;
  ambitionsDeclared: number;
  seizes: number;
}

export interface GameState {
  variant: string;
  players: number;
  chapter: number;
  phase: Phase;
  /** Holder of the initiative marker. */
  initiative: number;
  /** True once someone has seized it this round (it lies down). */
  initiativeSeized: boolean;
  systems: SystemState[];
  playerStates: PlayerState[];
  /** The general supply: 5 tokens of each resource type (p3). */
  supply: Record<ResourceType, number>;
  court: CourtSlot[];
  courtDeck: number[];
  courtDiscard: number[];
  actionDeck: number[];
  actionDiscard: number[];
  round: RoundState;
  turn: TurnState | null;
  battle: BattleState | null;
  move: MoveState | null;
  /** Ambition boxes → indices into `variant.ambitionMarkers`. */
  declared: Record<AmbitionId, number[]>;
  /** Markers still in the Available Markers area. */
  availableMarkers: number[];
  /** Markers flipped to their orange side (by marker index). */
  flipped: boolean[];
  /** 2-player only: phantom resources on ambition boxes (p19). */
  phantom: Record<AmbitionId, number>;
  /** Players still owing a reinforce decision at end of turn. */
  reinforcing: number | null;
  stats: GameStats;
}

// ---------------------------------------------------------------------------
// Actions
// ---------------------------------------------------------------------------

export type Action =
  // --- play phase ---
  | { t: 'lead'; card: number }
  | { t: 'follow'; card: number; mode: 'surpass' | 'copy' | 'pivot' }
  /** Initiative holder declines to play: initiative passes, round ends (p8). */
  | { t: 'passInitiative' }
  // --- mulligan ---
  | { t: 'mulligan'; take: boolean }
  // --- prelude ---
  | { t: 'declareAmbition'; ambition: AmbitionId }
  | { t: 'seize'; card: number }
  | { t: 'spendResource'; slot: number }
  | { t: 'beginActions' }
  // --- actions ---
  | { t: 'tax'; system: number; building: number }
  | { t: 'buildShip'; system: number; building: number }
  | { t: 'buildBuilding'; system: number; kind: BuildingKind }
  | { t: 'move'; from: number; to: number; ships: number }
  | { t: 'catapult'; to: number; ships: number }
  | { t: 'catapultStop' }
  | { t: 'repair'; system: number; building: number | null }
  | { t: 'influence'; slot: number }
  | { t: 'secure'; slot: number }
  | { t: 'battle'; system: number; defender: number; assault: number; skirmish: number; raid: number }
  | { t: 'endTurn' }
  // --- battle assignment ---
  /** Assign one self-hit to a Loyal ship in the battle system. */
  | { t: 'assignSelf'; fresh: boolean }
  /** Assign one hit to a defending ship (or building once ships are gone). */
  | { t: 'assignHit'; target: 'ship'; fresh: boolean }
  | { t: 'assignHit'; target: 'building'; building: number }
  /** Spend keys to steal a resource slot or a Guild card. */
  | { t: 'raidResource'; slot: number }
  | { t: 'raidCard'; card: number }
  | { t: 'raidDone' }
  // --- reinforce ---
  | { t: 'reinforce'; system: number };

export type PendingNode =
  | { kind: 'over' }
  | { kind: 'chance' }
  | { kind: 'decision'; player: number; actions: Action[] };
