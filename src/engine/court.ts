/**
 * The Court deck: 25 Guild cards + 6 Vox cards (rulebook p3, p4 step H, p17).
 *
 * A Guild card has a suit matching one of the 5 resources — it adds to Tycoon /
 * Keeper / Empath exactly like a resource token, and Weapon cards add to no
 * ambition (p17) — plus a raid cost and rules text. Vox cards resolve
 * immediately when secured and are discarded.
 *
 * Card names, suits, raid costs and verbatim text are transcribed from the
 * printed cards. Printed numbers run 01-25 for Guild and 26-31 for Vox, and
 * `id` is `number - 1`. Seven of these numbers are legible in the rulebook
 * itself (01 Loyal Engineers, 03 Material Cartel, 04 Admin Union, 09 Shipping
 * Interest, 11 Arms Union, 15 Loyal Marines, 18 Secret Order) and all seven
 * agree, which pins the ordering.
 *
 * `power` is the engine-readable form. A card with `power: undefined` is
 * data-complete but mechanically inert — see `UNIMPLEMENTED_POWERS` below and
 * docs/DATA-GAPS.md §4 for exactly which those are.
 */
import type { CourtCardDef, ResourceType, Suit } from './types';

// ---------------------------------------------------------------------------
// Card powers
// ---------------------------------------------------------------------------

/**
 * A Prelude ability that discards the card to do something (p20). Every one of
 * these reads "Prelude: You may discard this to ...".
 */
export type PreludeAbility =
  /** Place `n` fresh ships in one system you control. */
  | { t: 'placeShips'; count: number }
  /** Place 1 fresh ship in every in-play gate. */
  | { t: 'shipInEveryGate' }
  /** Gain `resource` up to your number of empty slots; steal if the supply is out. */
  | { t: 'fillSlots'; resource: ResourceType }
  /** Steal 1 `resource` from a Rival. */
  | { t: 'stealResource'; resource: ResourceType }
  /** Steal any one Guild card or resource from a Rival. */
  | { t: 'stealAny' }
  /** Gain one of each listed resource from the supply. */
  | { t: 'gainResources'; resources: ResourceType[] }
  /** Seize the initiative without paying a card. */
  | { t: 'seizeInitiative' }
  /** Once per turn, without discarding: trade 1 resource for 1 `gain`. */
  | { t: 'convertResource'; gain: ResourceType; keepCard: true };

/** A new action replacing a standard one, written `Name (Standard): …` (p20). */
export interface NewAction {
  name: string;
  /** The standard action whose pip pays for it. */
  replaces: 'build' | 'influence' | 'battle' | 'tax';
  effect:
    | { t: 'gainResource'; resource: ResourceType }
    | { t: 'pressgang' }
    | { t: 'execute' }
    | { t: 'abduct' }
    | { t: 'trade' };
}

/** Persistent effects the engine consults at specific points. */
export type Passive =
  /** "You may spend any resources as X", and Outrage never discards this card. */
  | { t: 'loyal'; as: ResourceType }
  /** Holds its resource type's supply; Rivals discard that type after scoring. */
  | { t: 'cartel'; resource: ResourceType }
  /** Attach to a played card of `suit`; draw that card at the end of the round. */
  | { t: 'union'; suit: Suit }
  /** Collect `count` extra battle dice when the battle system is a gate. */
  | { t: 'gateDice'; count: number }
  /** Reroll skirmish dice up to your Weapon icon count. */
  | { t: 'rerollSkirmish' }
  /** Rivals cannot steal your resources or other Guild cards. */
  | { t: 'theftImmune' }
  /** Declaring these ambitions does not place the zero marker. */
  | { t: 'noZeroMarker'; ambitions: 'keeperEmpath' | 'surpassPivot' }
  /** On declaring an ambition, look at a Rival hand and may swap 1 card. */
  | { t: 'peekAndSwap' };

export interface CardPower {
  prelude?: PreludeAbility;
  newActions?: NewAction[];
  passives?: Passive[];
}

/** What a Vox card does when secured (they always discard afterwards). */
export type VoxEffect =
  | { t: 'shipInEachSystemOfCluster' }
  | { t: 'declareAnyAmbition' }
  | { t: 'outrageAll' }
  | { t: 'returnCityMaySeize'; shuffleBackIn: true }
  | { t: 'stealGuildCardAndRecycle' }
  | { t: 'drawFromDiscardBottom' };

// ---------------------------------------------------------------------------
// The deck
// ---------------------------------------------------------------------------

const M: ResourceType = 'material';
const F: ResourceType = 'fuel';
const W: ResourceType = 'weapon';
const P: ResourceType = 'psionic';
const R: ResourceType = 'relic';

interface GuildSpec {
  name: string;
  suit: ResourceType;
  raidCost: number;
  text: string;
  power?: CardPower;
}

/** The "Loyal X" cards (01/07/15/19/21) share one shape. */
function loyal(name: string, suit: ResourceType, extra?: CardPower): GuildSpec {
  const label = { material: 'Material', fuel: 'Fuel', weapon: 'Weapons', psionic: 'Psionic', relic: 'Relics' }[suit];
  const single = { material: 'Material', fuel: 'Fuel', weapon: 'Weapons', psionic: 'Psionic', relic: 'Relics' }[suit];
  return {
    name,
    suit,
    raidCost: 3,
    text:
      `If you Provoke Outrage, keep this card. You may spend any resources as ${label}. ` +
      `You ignore Outrage when spending ${single} for ${suit === 'weapon' || suit === 'relic' ? 'their' : 'its'} Prelude action.` +
      (extra?.prelude ? ' Prelude: You may discard this to place 3 ships in a system you control.' : ''),
    power: { passives: [{ t: 'loyal', as: suit }], ...extra },
  };
}

/** The four "Union" cards (04/05/10/11) share one shape. */
function union(name: string, suit: ResourceType, cardSuit: Suit, label: string): GuildSpec {
  return {
    name,
    suit,
    raidCost: 2,
    text:
      `Prelude: You may place this card next to a face-up played ${label} card. ` +
      `When the round ends, draw that card into your hand and discard this card.`,
    power: { passives: [{ t: 'union', suit: cardSuit }] },
  };
}

/** The two "Cartel" cards (03/06) share one shape. */
function cartel(name: string, resource: ResourceType, label: string): GuildSpec {
  return {
    name,
    suit: resource,
    raidCost: 2,
    text:
      `You keep the ${label} supply on here. (You add it to Tycoon but can't spend it.) ` +
      `After scoring, Rivals discard all ${label}. Prelude: You may discard this to steal 1 ${label}.`,
    power: {
      passives: [{ t: 'cartel', resource }],
      prelude: { t: 'stealResource', resource },
    },
  };
}

/** The two "Interest" cards (02/09) share one shape. */
function interest(name: string, resource: ResourceType, action: string, label: string): GuildSpec {
  return {
    name,
    suit: resource,
    raidCost: 2,
    text:
      `${action} (Build): Gain 1 ${label}. Prelude: You may discard this to gain ${label} up to your ` +
      `number of empty resource slots. If the ${label} supply empties, steal the ${label} instead.`,
    power: {
      newActions: [{ name: action, replaces: 'build', effect: { t: 'gainResource', resource } }],
      prelude: { t: 'fillSlots', resource },
    },
  };
}

const PLACE_3_SHIPS: CardPower = { prelude: { t: 'placeShips', count: 3 } };

const GUILD_SPECS: GuildSpec[] = [
  loyal('Loyal Engineers', M),
  interest('Mining Interest', M, 'Manufacture', 'Material'),
  cartel('Material Cartel', M, 'Material'),
  union('Admin Union', M, 'administration', 'Administration'),
  union('Construction Union', M, 'construction', 'Construction'),
  cartel('Fuel Cartel', F, 'Fuel'),
  loyal('Loyal Pilots', F),
  {
    name: 'Gatekeepers',
    suit: F,
    raidCost: 2,
    text:
      'When you battle in a gate, you may collect 2 more dice. Prelude: You may discard this to ' +
      'place 1 ship in each gate (unless out of play).',
    power: { passives: [{ t: 'gateDice', count: 2 }], prelude: { t: 'shipInEveryGate' } },
  },
  interest('Shipping Interest', F, 'Synthesize', 'Fuel'),
  union('Spacing Union', F, 'mobilization', 'Mobilization'),
  union('Arms Union', W, 'aggression', 'Aggression'),
  {
    name: 'Prison Wardens',
    suit: W,
    raidCost: 2,
    text:
      'Pressgang (Build): Return any number of your Captives to gain any 1 resource for each. ' +
      'Execute (Influence): Move any number of your Captives to your Trophies. ' +
      'Prelude: You may discard this to place 3 ships in a system you control.',
    power: {
      newActions: [
        { name: 'Pressgang', replaces: 'build', effect: { t: 'pressgang' } },
        { name: 'Execute', replaces: 'influence', effect: { t: 'execute' } },
      ],
      ...PLACE_3_SHIPS,
    },
  },
  {
    name: 'Skirmishers',
    suit: W,
    raidCost: 2,
    text:
      'After you roll in battle, you may reroll a number of skirmish dice up to your total Weapon ' +
      'icons from resources and cards. Prelude: You may discard this to place 3 ships in a system you control.',
    power: { passives: [{ t: 'rerollSkirmish' }], ...PLACE_3_SHIPS },
  },
  {
    name: 'Court Enforcers',
    suit: W,
    raidCost: 2,
    text:
      'Abduct (Battle): Capture all Rival agents from a card in the Court that has fewer Rival agents ' +
      'than your total Weapon icons from resources and cards. ' +
      'Prelude: You may discard this to place 3 ships in a system you control.',
    power: {
      newActions: [{ name: 'Abduct', replaces: 'battle', effect: { t: 'abduct' } }],
      ...PLACE_3_SHIPS,
    },
  },
  loyal('Loyal Marines', W, PLACE_3_SHIPS),
  {
    name: 'Lattice Spies',
    suit: P,
    raidCost: 2,
    text:
      'Prelude: Before any other actions, you may discard this to seize the initiative. ' +
      "You can't seize the initiative if you have it or if it's already seized this round!",
    power: { prelude: { t: 'seizeInitiative' } },
  },
  {
    name: 'Farseers',
    suit: P,
    raidCost: 2,
    text:
      "When you declare an ambition, look at a Rival's hand. You may swap 1 card with them. " +
      'Prelude: You may discard this and any number of cards from your hand. Draw the same number ' +
      'of cards (including Farseers) from the bottom of the action discard pile.',
    power: { passives: [{ t: 'peekAndSwap' }] },
  },
  {
    name: 'Secret Order',
    suit: P,
    raidCost: 2,
    text: 'When you declare Keeper or Empath, do not place the zero marker.',
    power: { passives: [{ t: 'noZeroMarker', ambitions: 'keeperEmpath' }] },
  },
  loyal('Loyal Empaths', P),
  {
    name: 'Silver-Tongues',
    suit: P,
    raidCost: 2,
    text: 'Prelude: You may discard this to steal a Guild card or resource.',
    power: { prelude: { t: 'stealAny' } },
  },
  loyal('Loyal Keepers', R),
  {
    name: 'Sworn Guardians',
    suit: R,
    raidCost: 1,
    text:
      'Rivals cannot steal your resources and other Guild cards. (In battle they can steal this first ' +
      'and then spend keys.) If this card is stolen, bury it. (Place it on the bottom of the Court deck.)',
    power: { passives: [{ t: 'theftImmune' }] },
  },
  {
    name: 'Elder Broker',
    suit: R,
    raidCost: 2,
    text:
      'Trade (Tax): Choose a Rival city you control. Swap 1 resource with that Rival—take a resource ' +
      "of that city type from them, and give them a resource they don't have. " +
      'Prelude: You may discard this to gain 1 Material, 1 Fuel, and 1 Weapon.',
    power: {
      newActions: [{ name: 'Trade', replaces: 'tax', effect: { t: 'trade' } }],
      prelude: { t: 'gainResources', resources: [M, F, W] },
    },
  },
  {
    name: 'Relic Fence',
    suit: R,
    raidCost: 2,
    text: 'Prelude: Once per turn, you may discard 1 resource to gain 1 Relic.',
    power: { prelude: { t: 'convertResource', gain: R, keepCard: true } },
  },
  {
    name: 'Galactic Bards',
    suit: R,
    raidCost: 1,
    text:
      'When you Surpass or Pivot, before taking any actions, you may declare the ambition on your ' +
      'played card if an ambition has not been declared yet this round. Do not place the zero marker.',
    power: { passives: [{ t: 'noZeroMarker', ambitions: 'surpassPivot' }] },
  },
];

interface VoxSpec {
  name: string;
  text: string;
  effect: VoxEffect;
}

const VOX_SPECS: VoxSpec[] = [
  {
    name: 'Mass Uprising',
    text:
      'When Secured: Choose a cluster on the map. You place 1 ship in each system of that cluster. ' +
      'Discard this card.',
    effect: { t: 'shipInEachSystemOfCluster' },
  },
  {
    name: 'Populist Demands',
    text:
      'When Secured: You may declare any ambition. (An ambition marker must be Available.) Discard this card.',
    effect: { t: 'declareAnyAmbition' },
  },
  {
    name: 'Outrage Spreads',
    text:
      'When Secured: You may choose an Outrage type. Each player (even you) must Provoke Outrage of ' +
      'that type. Discard this card.',
    effect: { t: 'outrageAll' },
  },
  {
    name: 'Song of Freedom',
    text:
      'When Secured: You may return any one city you control to its player board. If you do, you may ' +
      'seize the initiative. Shuffle this card into the Court deck.',
    effect: { t: 'returnCityMaySeize', shuffleBackIn: true },
  },
  {
    name: 'Guild Struggle',
    text:
      'When Secured: You may steal 1 Guild card from a Rival. Shuffle all Guild cards from the Court ' +
      'discard pile into the Court deck. Discard Guild Struggle.',
    effect: { t: 'stealGuildCardAndRecycle' },
  },
  {
    name: 'Call to Action',
    text:
      'When Secured: Draw 1 action card from the bottom of the action discard pile. Discard Call to Action.',
    effect: { t: 'drawFromDiscardBottom' },
  },
];

export const GUILD_CARDS: CourtCardDef[] = GUILD_SPECS.map((spec, i) => ({
  id: i,
  number: i + 1,
  name: spec.name,
  kind: 'guild' as const,
  suit: spec.suit,
  raidCost: spec.raidCost,
  text: spec.text,
  power: spec.power,
}));

export const VOX_CARDS: CourtCardDef[] = VOX_SPECS.map((spec, i) => ({
  id: GUILD_CARDS.length + i,
  number: GUILD_CARDS.length + i + 1,
  name: spec.name,
  kind: 'vox' as const,
  suit: null,
  raidCost: 0,
  text: spec.text,
  vox: spec.effect,
  discardOnSecure: true,
}));

export const COURT_DECK: CourtCardDef[] = [...GUILD_CARDS, ...VOX_CARDS];

export function courtCard(id: number): CourtCardDef {
  return COURT_DECK[id];
}

/** Court row width: 3 at 2 players, 4 at 3-4 players (p4 step H). */
export function courtRowSize(players: number): number {
  return players === 2 ? 3 : 4;
}

/**
 * Cards whose printed ability the engine actually dispatches. Add a name here
 * only when its power is wired up *and* covered by a test.
 *
 * Everything else about every card — suit, raid cost, being influenced,
 * secured, stolen, discarded by Outrage and counted toward ambitions — already
 * works; only the printed ability is inert. Tracking this as data rather than
 * a comment lets a test assert the README's claim stays true.
 */
export const IMPLEMENTED_POWERS = new Set<string>([]);

/** The complement: cards carrying an ability the engine does not act on yet. */
export const UNIMPLEMENTED_POWERS: string[] = COURT_DECK.filter(
  (c) => (c.power !== undefined || c.vox !== undefined) && !IMPLEMENTED_POWERS.has(c.name),
).map((c) => c.name);
