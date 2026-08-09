/**
 * The Arcs state machine.
 *
 * The game advances through nodes reported by `getPending()`:
 *
 *   { kind: 'chance' }   → resolveChanceMut(state, variant, rng)
 *   { kind: 'decision' } → choose one of `actions`, apply with applyAction[Mut]
 *   { kind: 'over' }     → read final Power with `standings()`
 *
 * `applyAction` / `resolveChance` clone; the `*Mut` versions mutate and exist
 * for hot loops (simulation, MCTS rollouts).
 *
 * Two engine rulings, where the rulebook leaves a choice the state machine
 * would otherwise have to surface as its own phase (both noted in the README):
 *
 *   - **Ransacking the Court** (p16) picks the Court card holding the most of
 *     the defender's agents, breaking ties leftmost.
 *   - **Free actions from Prelude resources** are consumed before card pips,
 *     most-restrictive grant first, when both could pay for an action.
 *
 * A Vox card secured mid-turn or mid-Ransack overlays `pendingVox` on the phase
 * rather than replacing it, so the interrupted flow resumes afterwards. Both
 * `afterAction` and `settleBattle` stall while one is pending.
 */
import {
  cartelHolder,
  cloneState,
  controlOf,
  freeBuildingSlots,
  hasAnyBuilding,
  hasBuilding,
  hasPiece,
  isWipedOut,
  neighbours,
  returnToSupply,
  rivalsIn,
  shipsOf,
  takeFromSupply,
} from './board';
import { actionCard, SUIT_ACTIONS } from './cards';
import { courtCard } from './court';
import { DICE_PER_TYPE, rerollSkirmish, rollBattle, SKIRMISH_FACES, type RollTotals } from './dice';
import { flipLowestUnflipped, highestAvailable, scoreChapter } from './ambitions';
import { isGate } from './map';
import { compactResources, heldResources, openResourceSlots, raidCost } from './playerBoard';
import {
  applyCardAction,
  applyCardPrelude,
  applyPeekSwap,
  applyVox,
  applyVoxImmediate,
  canDeclareOnFollow,
  canPeek,
  canRerollSkirmish,
  cardActionCost,
  cardActions,
  discardGuildCard,
  extraBattleDice,
  gainGuildCard,
  loyalTypes,
  peekSwapActions,
  peekTargetActions,
  preludeCardActions,
  provokeOutrage,
  skipsZeroMarker,
  stealGuildCard,
  theftImmune,
  theftImmunityCard,
  voxActions,
  voxNeedsDecision,
  weaponIcons,
} from './powers';
import { shuffle } from './rng';
import type {
  Action,
  ActionKind,
  AmbitionId,
  BuildingKind,
  DieType,
  GameState,
  PendingNode,
  Phase,
  PlayMode,
  ResourceType,
  RNG,
  VariantDef,
} from './types';
import { AMBITIONS, RESOURCE_TYPES } from './types';

export { cloneState };

// ---------------------------------------------------------------------------
// Node inspection
// ---------------------------------------------------------------------------

export function getPending(s: GameState, v: VariantDef): PendingNode {
  // A pending Vox effect belongs to whoever secured the card, and outranks the
  // phase it interrupted.
  if (s.pendingVox) {
    return { kind: 'decision', player: s.pendingVox.player, actions: legalActions(s, v) };
  }
  switch (s.phase) {
    case 'over':
      return { kind: 'over' };
    case 'deal':
    case 'battleRoll':
      return { kind: 'chance' };
    case 'mulligan':
      return { kind: 'decision', player: mulliganPlayer(s), actions: legalActions(s, v) };
    case 'reinforce':
      return { kind: 'decision', player: s.reinforcing!, actions: legalActions(s, v) };
    case 'play':
      return { kind: 'decision', player: currentActor(s), actions: legalActions(s, v) };
    case 'peekTarget':
    case 'peekSwap':
      return { kind: 'decision', player: s.peek!.player, actions: legalActions(s, v) };
    default:
      return { kind: 'decision', player: s.turn!.player, actions: legalActions(s, v) };
  }
}

function currentActor(s: GameState): number {
  return s.round.turnOrder[s.round.turnIndex];
}

/** 2-player mulligan belongs to the player without initiative (p19). */
function mulliganPlayer(s: GameState): number {
  return (s.initiative + 1) % s.players;
}

// ---------------------------------------------------------------------------
// Legal actions
// ---------------------------------------------------------------------------

export function legalActions(s: GameState, v: VariantDef): Action[] {
  if (s.pendingVox) return voxActions(s);
  switch (s.phase) {
    case 'mulligan':
      return [
        { t: 'mulligan', take: false },
        { t: 'mulligan', take: true },
      ];
    case 'play':
      return playActions(s, v);
    case 'prelude':
      return preludeActions(s, v);
    case 'actions':
      return turnActions(s, v);
    case 'catapult':
      return catapultActions(s, v);
    case 'battleReroll':
      return rerollActions(s);
    case 'battleAssign':
      return battleActions(s, v);
    case 'peekTarget':
      return peekTargetActions(s, s.peek!.player);
    case 'peekSwap':
      return peekSwapActions(s);
    case 'reinforce':
      return v.systems
        .filter((sys) => sys.kind === 'gate' && !s.systems[sys.id].outOfPlay)
        .map((sys) => ({ t: 'reinforce', system: sys.id }) as Action);
    default:
      throw new Error(`no legal actions in phase ${s.phase}`);
  }
}

/** Step 1 / Step 2: lead, or surpass / copy / pivot (p8, p10). */
function playActions(s: GameState, _v: VariantDef): Action[] {
  const player = currentActor(s);
  const hand = s.playerStates[player].hand;
  const acts: Action[] = [];

  if (s.round.turnIndex === 0) {
    for (const card of hand) acts.push({ t: 'lead', card });
    acts.push({ t: 'passInitiative' });
    return acts;
  }

  const lead = s.round.lead!;
  const leadSuit = actionCard(lead.card).suit;
  for (const card of hand) {
    const def = actionCard(card);
    if (def.suit === leadSuit && def.number > s.round.leadNumber) {
      acts.push({ t: 'follow', card, mode: 'surpass' });
    }
    if (def.suit !== leadSuit) {
      acts.push({ t: 'follow', card, mode: 'pivot' });
    }
    acts.push({ t: 'follow', card, mode: 'copy' });
  }
  return acts;
}

/** Declare / seize / spend resources, then begin actions (p9, p10, p17, p20). */
function preludeActions(s: GameState, v: VariantDef): Action[] {
  const turn = s.turn!;
  const p = s.playerStates[turn.player];
  const acts: Action[] = [];

  // Declaring and seizing are decided before any Prelude action (p20).
  if (!turn.preludeOver && turn.preludeSpent.length === 0) {
    for (const ambition of declarableAmbitions(s)) {
      acts.push({ t: 'declareAmbition', ambition });
    }
    if (canSeize(s, turn.player)) {
      for (const card of p.hand) acts.push({ t: 'seize', card });
    }
  }

  // Spend resources for their Prelude actions (p17). Outraged types cannot.
  const seen = new Set<ResourceType>();
  for (let i = 0; i < openResourceSlots(p); i++) {
    const r = p.resources[i];
    if (!r || seen.has(r)) continue;
    seen.add(r);
    if (!p.outrage[r] && !(r === 'psionic' && !s.round.lead)) {
      acts.push({ t: 'spendResource', slot: i });
    }
    // A Loyal card lets any resource be spent as its type, ignoring Outrage
    // on the type being spent as (p20 rules hierarchy, "ignore").
    for (const as of loyalTypes(p)) {
      if (as === r) continue;
      if (as === 'psionic' && !s.round.lead) continue;
      acts.push({ t: 'spendResourceAs', slot: i, as });
    }
  }

  acts.push(...preludeCardActions(s, v, turn.player));
  acts.push({ t: 'beginActions' });
  return acts;
}

/**
 * Which ambitions the current turn may declare.
 *
 * The normal route is the leader declaring the ambition printed on a face-up
 * lead card (p9). **Galactic Bards** adds a second: "When you Surpass or Pivot,
 * before taking any actions, you may declare the ambition on your played card if
 * an ambition has not been declared yet this round."
 */
function declarableAmbitions(s: GameState): AmbitionId[] {
  const turn = s.turn!;
  if (turn.declaredThisTurn || s.availableMarkers.length === 0) return [];

  let card: number | null = null;
  if (turn.mode === 'lead') {
    if (!s.round.lead || s.round.lead.faceDown) return [];
    card = s.round.lead.card;
  } else if (turn.mode === 'surpass' || turn.mode === 'pivot') {
    if (!canDeclareOnFollow(s.playerStates[turn.player])) return [];
    if (s.round.ambitionDeclared) return [];
    card = turn.card;
  } else {
    return []; // a Copy play is face down and declares nothing
  }

  const ambition = actionCard(card).ambition;
  if (ambition === 'any') return [...AMBITIONS];
  return ambition ? [ambition] : [];
}

/**
 * "You cannot do this if you have the initiative marker or if someone has
 * already seized the initiative this round" (p23, glossary).
 *
 * This is the eligibility test alone. The two *routes* have different costs:
 * playing an extra card face down needs a spare card in hand (checked where
 * the seize actions are enumerated), while Surpassing with a "7" needs nothing
 * beyond the card you were already playing.
 */
function canSeize(s: GameState, player: number): boolean {
  return !s.initiativeSeized && player !== s.initiative;
}

/** Which action kinds the current turn can still pay for. */
function availableKinds(s: GameState): ActionKind[] {
  const turn = s.turn!;
  const kinds = new Set<ActionKind>();
  if (turn.pipsLeft > 0) {
    for (const k of turn.pipActions) kinds.add(k);
    if (turn.weaponSpent) kinds.add('battle');
  }
  for (const grant of turn.freeActions) for (const k of grant) kinds.add(k);
  return [...kinds];
}

function turnActions(s: GameState, v: VariantDef): Action[] {
  const turn = s.turn!;
  const player = turn.player;
  const acts: Action[] = [];
  const kinds = new Set(availableKinds(s));

  if (kinds.has('tax')) acts.push(...taxActions(s, v, player));
  if (kinds.has('build')) acts.push(...buildActions(s, v, player));
  if (kinds.has('move')) acts.push(...moveActions(s, v, player));
  if (kinds.has('repair')) acts.push(...repairActions(s, player));
  if (kinds.has('influence')) acts.push(...influenceActions(s, player));
  if (kinds.has('secure')) acts.push(...secureActions(s, player));
  if (kinds.has('battle')) acts.push(...battleSetupActions(s, v, player));
  acts.push(...cardActions(s, v, player, kinds as Set<string>));

  acts.push({ t: 'endTurn' });
  return acts;
}

/** Tax: a Loyal city, or a Rival city in a system you control (p12). */
function taxActions(s: GameState, v: VariantDef, player: number): Action[] {
  const acts: Action[] = [];
  for (const def of v.systems) {
    const st = s.systems[def.id];
    if (st.outOfPlay) continue;
    const controller = controlOf(s, def.id);
    st.buildings.forEach((b, i) => {
      if (b.kind !== 'city' || b.taxedThisTurn) return;
      if (b.player !== player && controller !== player) return;
      acts.push({ t: 'tax', system: def.id, building: i });
    });
  }
  return acts;
}

/** Build a starport/city in an empty slot, or a ship at a Loyal starport (p12). */
function buildActions(s: GameState, v: VariantDef, player: number): Action[] {
  const acts: Action[] = [];
  const p = s.playerStates[player];
  for (const def of v.systems) {
    const st = s.systems[def.id];
    if (st.outOfPlay) continue;

    if (def.kind === 'planet' && hasPiece(s, def.id, player) && freeBuildingSlots(s, def) > 0) {
      const kinds: BuildingKind[] = [];
      if (p.starportsSupply > 0) kinds.push('starport');
      if (p.citiesUsed < 5) kinds.push('city');
      for (const kind of kinds) acts.push({ t: 'buildBuilding', system: def.id, kind });
    }

    if (p.shipsSupply > 0) {
      st.buildings.forEach((b, i) => {
        if (b.kind === 'starport' && b.player === player && !b.builtThisTurn) {
          acts.push({ t: 'buildShip', system: def.id, building: i });
        }
      });
    }
  }
  return acts;
}

/** Move any number of Loyal ships to an adjacent system (p13). */
function moveActions(s: GameState, v: VariantDef, player: number): Action[] {
  const acts: Action[] = [];
  for (let from = 0; from < s.systems.length; from++) {
    if (s.systems[from].outOfPlay) continue;
    const count = shipsOf(s, from, player);
    if (count === 0) continue;
    for (const to of neighbours(v, s, from)) {
      for (let n = 1; n <= count; n++) acts.push({ t: 'move', from, to, ships: n });
    }
  }
  return acts;
}

/** Repair 1 damaged Loyal ship or building, anywhere on the map (p13). */
function repairActions(s: GameState, player: number): Action[] {
  const acts: Action[] = [];
  for (let i = 0; i < s.systems.length; i++) {
    const st = s.systems[i];
    if (st.outOfPlay) continue;
    if (st.damaged[player] > 0) acts.push({ t: 'repair', system: i, building: null });
    st.buildings.forEach((b, bi) => {
      if (b.player === player && b.damaged) acts.push({ t: 'repair', system: i, building: bi });
    });
  }
  return acts;
}

/** Place 1 agent on any card in the Court (p13). */
function influenceActions(s: GameState, player: number): Action[] {
  if (s.playerStates[player].agentsSupply <= 0) return [];
  return s.court.map((_, slot) => ({ t: 'influence', slot }) as Action);
}

/** Take a Court card you have strictly the most agents on (p13). */
function secureActions(s: GameState, player: number): Action[] {
  const acts: Action[] = [];
  s.court.forEach((slot, i) => {
    const mine = slot.agents[player];
    if (mine <= 0) return;
    for (let p = 0; p < s.players; p++) {
      if (p !== player && slot.agents[p] >= mine) return;
    }
    acts.push({ t: 'secure', slot: i });
  });
  return acts;
}

/**
 * Battle: pick a system, a defender and the dice pool (p14). The dice split is
 * part of the action so the whole battle decision is one enumerable choice.
 */
function battleSetupActions(s: GameState, v: VariantDef, player: number): Action[] {
  const acts: Action[] = [];
  for (let system = 0; system < s.systems.length; system++) {
    if (s.systems[system].outOfPlay) continue;
    const attacking = shipsOf(s, system, player);
    if (attacking === 0) continue;
    for (const defender of rivalsIn(s, system, player)) {
      // "Raid dice only if there are defending buildings, or the defender has
      // no Loyal buildings in any systems on the map." (p14)
      const defendingBuildings = s.systems[system].buildings.some((b) => b.player === defender);
      const raidAllowed = defendingBuildings || !hasAnyBuilding(s, defender);
      // "1 die per attacking ship ... you cannot collect more than 6 of a type"
      const max = Math.min(attacking + extraBattleDice(s.playerStates[player], system), DICE_PER_TYPE * 3);
      for (let a = 0; a <= Math.min(DICE_PER_TYPE, max); a++) {
        for (let sk = 0; sk <= Math.min(DICE_PER_TYPE, max - a); sk++) {
          const maxRaid = raidAllowed ? Math.min(DICE_PER_TYPE, max - a - sk) : 0;
          for (let r = 0; r <= maxRaid; r++) {
            if (a + sk + r === 0) continue;
            acts.push({ t: 'battle', system, defender, assault: a, skirmish: sk, raid: r });
          }
        }
      }
    }
  }
  return acts;
}

function catapultActions(s: GameState, v: VariantDef): Action[] {
  const move = s.move!;
  const acts: Action[] = [{ t: 'catapultStop' }];
  for (const to of neighbours(v, s, move.at)) {
    if (move.visited.includes(to)) continue;
    for (let n = 1; n <= move.ships; n++) acts.push({ t: 'catapult', to, ships: n });
  }
  return acts;
}

/**
 * Skirmishers: "After you roll in battle, you may reroll a number of skirmish
 * dice up to your total Weapon icons from resources and cards."
 *
 * Engine ruling: only *blank* skirmish dice are offered. A skirmish face is
 * either one hit or nothing and the die never hurts the attacker, so rerolling
 * a face that already hit is strictly dominated — it can only lose a hit. Every
 * outcome reachable by rerolling a hit is reachable by rerolling fewer dice.
 */
function rerollActions(s: GameState): Action[] {
  const b = s.battle!;
  const most = Math.min(weaponIcons(s.playerStates[b.attacker]), b.skirmishBlanks);
  const acts: Action[] = [];
  for (let count = 0; count <= most; count++) acts.push({ t: 'rerollSkirmish', count });
  return acts;
}

/** Hits are assigned one at a time so the action space stays enumerable. */
function battleActions(s: GameState, v: VariantDef): Action[] {
  const b = s.battle!;
  const st = s.systems[b.system];
  const acts: Action[] = [];

  if (b.selfHits > 0) {
    if (st.fresh[b.attacker] > 0) acts.push({ t: 'assignSelf', fresh: true });
    if (st.damaged[b.attacker] > 0) acts.push({ t: 'assignSelf', fresh: false });
    if (acts.length > 0) return acts;
  }

  if (b.hits > 0) {
    if (st.fresh[b.defender] > 0) acts.push({ t: 'assignHit', target: 'ship', fresh: true });
    if (st.damaged[b.defender] > 0) acts.push({ t: 'assignHit', target: 'ship', fresh: false });
    if (acts.length === 0) {
      st.buildings.forEach((bl, i) => {
        if (bl.player === b.defender) acts.push({ t: 'assignHit', target: 'building', building: i });
      });
    }
    if (acts.length > 0) return acts;
  }

  if (b.buildingHits > 0) {
    st.buildings.forEach((bl, i) => {
      if (bl.player === b.defender) acts.push({ t: 'assignHit', target: 'building', building: i });
    });
    if (acts.length > 0) return acts;
  }

  // Raiding needs surviving attacking ships (p14 step 4.5).
  if (b.keys > 0 && shipsOf(s, b.system, b.attacker) > 0) {
    const victim = s.playerStates[b.defender];
    // Sworn Guardians: "Rivals cannot steal your resources and other Guild
    // cards. (In battle they can steal this first and then spend keys.)"
    const shield = theftImmune(victim) ? theftImmunityCard(victim) : null;
    if (shield !== null) {
      if (courtCard(shield).raidCost <= b.keys) acts.push({ t: 'raidCard', card: shield });
    } else {
      for (let slot = 0; slot < openResourceSlots(victim); slot++) {
        if (victim.resources[slot] && raidCost(slot) <= b.keys) {
          acts.push({ t: 'raidResource', slot });
        }
      }
      for (const card of victim.guildCards) {
        if (courtCard(card).raidCost <= b.keys) acts.push({ t: 'raidCard', card });
      }
    }
    acts.push({ t: 'raidDone' });
    return acts;
  }

  return [{ t: 'raidDone' }];
}

// ---------------------------------------------------------------------------
// Chance
// ---------------------------------------------------------------------------

export function resolveChanceMut(s: GameState, v: VariantDef, rng: RNG): void {
  if (s.phase === 'deal') {
    dealChapter(s, v, rng);
    return;
  }
  if (s.phase === 'battleRoll') {
    const b = s.battle!;

    // A Skirmishers reroll replaces only the blank skirmish dice it names; the
    // rest of the roll stands, and no new intercept can appear because the
    // skirmish die has no intercept face.
    if (b.pendingReroll > 0) {
      const { hits, blanks, faces } = rerollSkirmish(b.pendingReroll, rng);
      // Swap the named blanks for their new faces; the rest of the roll stands.
      let toSwap = b.pendingReroll;
      b.rolled.skirmish = b.rolled.skirmish.filter((idx) => {
        if (toSwap > 0 && SKIRMISH_FACES[idx].hits === 0) {
          toSwap--;
          return false;
        }
        return true;
      });
      b.rolled.skirmish.push(...faces);
      b.hits += hits;
      b.skirmishBlanks = b.skirmishBlanks - b.pendingReroll + blanks;
      b.pendingReroll = 0;
      b.rerollDone = true;
      s.phase = 'battleAssign';
      settleBattle(s, v);
      return;
    }

    const roll = rollBattle(b.dice, rng);
    // The faces are what the table shows; a bot applying a synthetic
    // outcome via applyBattleRollMut has no faces and skips this.
    b.rolled = roll.faces;
    applyBattleRollMut(s, v, roll);
    return;
  }
  throw new Error(`resolveChance in phase ${s.phase}`);
}

export function resolveChance(s: GameState, v: VariantDef, rng: RNG): GameState {
  const ns = cloneState(s);
  resolveChanceMut(ns, v, rng);
  return ns;
}

/**
 * The deterministic half of the `battleRoll` chance node: apply a roll's
 * totals and advance the battle. Split from `resolveChanceMut` (which feeds
 * it `rollBattle`) so a bot can value a battle as the exact expectation over
 * `battleDistribution` outcomes instead of sampling rolls.
 */
export function applyBattleRollMut(s: GameState, v: VariantDef, totals: RollTotals): void {
  const b = s.battle!;
  b.selfHits = totals.selfHits;
  b.intercept = totals.intercept;
  b.hits = totals.hits;
  b.buildingHits = totals.buildingHits;
  b.keys = totals.keys;
  b.skirmishBlanks = totals.skirmishBlanks;
  // Step 4.2: the intercept converts into self-hits, once per battle (p14).
  if (b.intercept > 0) {
    b.selfHits += s.systems[b.system].fresh[b.defender];
    b.interceptResolved = true;
  }
  if (
    !b.rerollDone &&
    b.skirmishBlanks > 0 &&
    canRerollSkirmish(s.playerStates[b.attacker]) &&
    weaponIcons(s.playerStates[b.attacker]) > 0
  ) {
    s.phase = 'battleReroll';
    return;
  }
  s.phase = 'battleAssign';
  settleBattle(s, v);
}

/** Step 4 of ending a chapter: shuffle everything, deal 6 each (p19). */
function dealChapter(s: GameState, v: VariantDef, rng: RNG): void {
  const deck = shuffle([...s.actionDeck, ...s.actionDiscard], rng);
  for (const p of s.playerStates) {
    for (const card of p.hand) deck.push(card);
    p.hand = [];
  }
  shuffle(deck, rng);
  for (let i = 0; i < v.players; i++) {
    s.playerStates[i].hand = deck.splice(0, v.handSize);
  }
  s.actionDeck = [];
  s.actionDiscard = deck;
  s.round.consecutivePasses = 0;
  // A fresh deal makes every card's location unknown again.
  s.revealed = [];
  s.declines = [];

  if (v.players === 2) {
    s.phase = 'mulligan';
    return;
  }
  beginRound(s, v);
}

// ---------------------------------------------------------------------------
// Transitions
// ---------------------------------------------------------------------------

export function applyAction(s: GameState, v: VariantDef, a: Action): GameState {
  const ns = cloneState(s);
  applyActionMut(ns, v, a);
  return ns;
}

export function applyActionMut(s: GameState, v: VariantDef, a: Action): void {
  // A pending Vox effect is resolved before anything else can happen.
  if (s.pendingVox) {
    resolveVox(s, v, a);
    return;
  }

  switch (a.t) {
    case 'rerollSkirmish': {
      const b = s.battle!;
      if (a.count > 0) {
        b.pendingReroll = a.count;
        s.phase = 'battleRoll';
        return;
      }
      b.rerollDone = true;
      s.phase = 'battleAssign';
      settleBattle(s, v);
      return;
    }
    case 'peekTarget': {
      const peek = s.peek!;
      if (a.target === null) {
        endPeek(s, v);
        return;
      }
      peek.target = a.target;
      s.phase = 'peekSwap';
      return;
    }
    case 'peekSwap':
    case 'peekSwapSkip': {
      applyPeekSwap(s, a);
      endPeek(s, v);
      return;
    }
    case 'mulligan': {
      const player = mulliganPlayer(s);
      if (a.take) {
        const p = s.playerStates[player];
        s.actionDiscard.push(...p.hand);
        p.hand = s.actionDiscard.splice(0, v.handSize);
      }
      beginRound(s, v);
      return;
    }
    case 'lead':
    case 'follow': {
      playCard(s, v, a);
      return;
    }
    case 'passInitiative': {
      passInitiative(s, v);
      return;
    }
    case 'declareAmbition': {
      const turn = s.turn!;
      declareAmbition(s, v, a.ambition, turn.player, turn.mode);
      offerPeek(s, turn.player, 'prelude');
      return;
    }
    case 'seize': {
      const turn = s.turn!;
      const p = s.playerStates[turn.player];
      p.hand.splice(p.hand.indexOf(a.card), 1);
      s.round.played.push({ player: turn.player, card: a.card, mode: turn.mode, faceDown: true });
      s.round.seizedBy = turn.player;
      s.initiativeSeized = true;
      s.stats.seizes++;
      return;
    }
    case 'spendResource': {
      spendResource(s, a.slot);
      return;
    }
    case 'spendResourceAs': {
      // A Loyal card: the token leaves your board, but its Prelude action is
      // the one printed for the type you are spending it as.
      spendResource(s, a.slot, a.as);
      return;
    }
    case 'cardPrelude': {
      applyCardPrelude(s, v, a);
      return;
    }
    case 'beginActions': {
      endPrelude(s);
      return;
    }
    case 'endTurn': {
      endTurn(s, v);
      return;
    }
    case 'reinforce': {
      const player = s.reinforcing!;
      const n = Math.min(3, s.playerStates[player].shipsSupply);
      s.systems[a.system].fresh[player] += n;
      s.playerStates[player].shipsSupply -= n;
      s.reinforcing = null;
      finishTurn(s, v);
      return;
    }
    case 'catapult': {
      const move = s.move!;
      moveShipsInto(s, v, move.at, a.to, s.turn!.player, a.ships);
      return;
    }
    case 'catapultStop': {
      s.move = null;
      s.phase = 'actions';
      afterAction(s, v);
      return;
    }
    default:
      break;
  }

  // Battle assignment and raiding run inside a battle, not against pips.
  if (s.phase === 'battleAssign') {
    battleAssign(s, v, a);
    return;
  }

  performAction(s, v, a);
}

// --- playing a card -------------------------------------------------------

function playCard(s: GameState, v: VariantDef, a: Action): void {
  const player = currentActor(s);
  const p = s.playerStates[player];
  const card = a.t === 'lead' ? a.card : (a as { card: number }).card;
  const mode: PlayMode = a.t === 'lead' ? 'lead' : (a as { mode: PlayMode }).mode;
  const def = actionCard(card);

  // Public memory: a follower who does not Surpass is telling the table
  // something about their hand. Record it before the lead can change.
  if (mode === 'copy' || mode === 'pivot') {
    const lead = s.round.lead;
    if (lead) {
      s.declines.push({
        player,
        suit: actionCard(lead.card).suit,
        number: s.round.leadNumber,
      });
    }
  }

  p.hand.splice(p.hand.indexOf(card), 1);
  const played = { player, card, mode, faceDown: mode === 'copy' };
  s.round.played.push(played);
  s.stats.cardsPlayed++;

  let pips: number;
  let pipActions: ActionKind[];
  if (mode === 'lead') {
    s.round.lead = played;
    s.round.leadNumber = def.number;
    s.round.consecutivePasses = 0;
    pips = def.pips;
    pipActions = SUIT_ACTIONS[def.suit];
  } else if (mode === 'surpass') {
    pips = def.pips;
    pipActions = SUIT_ACTIONS[def.suit];
    // "You Surpass with a 7 action card" seizes the initiative (p10).
    if (def.number === 7 && canSeize(s, player)) {
      s.round.seizedBy = player;
      s.initiativeSeized = true;
      s.stats.seizes++;
    }
  } else if (mode === 'pivot') {
    pips = 1;
    pipActions = SUIT_ACTIONS[def.suit];
  } else {
    pips = 1;
    pipActions = SUIT_ACTIONS[actionCard(s.round.lead!.card).suit];
  }

  s.turn = {
    player,
    mode,
    card,
    pipsLeft: pips,
    pipActions: [...pipActions],
    freeActions: [],
    weaponSpent: false,
    preludeOver: false,
    declaredThisTurn: false,
    preludeSpent: [],
    securedThisPrelude: [],
    cardPreludesUsed: [],
  };
  s.phase = 'prelude';
}

/**
 * Declare an ambition (p9). `player` is the declarer, which is the player on
 * turn for the normal route but can be a securing player for Populist Demands.
 */
function declareAmbition(
  s: GameState,
  v: VariantDef,
  ambition: AmbitionId,
  player: number,
  mode: PlayMode,
): void {
  const marker = highestAvailable(s, v);
  if (marker === null) throw new Error('no ambition marker available');
  s.availableMarkers.splice(s.availableMarkers.indexOf(marker), 1);
  s.declared[ambition].push(marker);
  if (s.turn && s.turn.player === player) s.turn.declaredThisTurn = true;
  s.round.ambitionDeclared = true;
  // "Place the zero marker onto the lead card ... its card number is now 0."
  // Secret Order exempts Keeper and Empath; Galactic Bards exempts its own
  // Surpass/Pivot declaration.
  if (!skipsZeroMarker(s.playerStates[player], ambition, mode)) s.round.leadNumber = 0;
  s.stats.ambitionsDeclared++;
}

/**
 * Farseers: "When you declare an ambition, look at a Rival's hand." Opens the
 * two-step peek, returning to `resume` afterwards. Returns whether it opened.
 */
function offerPeek(s: GameState, player: number, resume: Phase): boolean {
  if (!canPeek(s.playerStates[player])) return false;
  const hasTarget = s.playerStates.some((p, i) => i !== player && p.hand.length > 0);
  if (!hasTarget) return false;
  s.peek = { player, target: null, resume };
  s.phase = 'peekTarget';
  return true;
}

/**
 * Close a Farseers peek and continue whatever it interrupted. The prelude just
 * carries on; a battle or a turn needs its own continuation re-run, since the
 * interrupted call already returned.
 */
function endPeek(s: GameState, v: VariantDef): void {
  const resume = s.peek!.resume;
  s.peek = null;
  s.phase = resume;
  if (resume === 'battleAssign') settleBattle(s, v);
  else if (resume === 'actions') afterAction(s, v);
}

/**
 * Resolve a pending Vox `When Secured` effect, then resume the flow it
 * interrupted — the rest of the turn's actions, or the rest of the battle.
 */
function resolveVox(s: GameState, v: VariantDef, a: Action): void {
  const pending = s.pendingVox!;
  const player = pending.player;
  const resume: Phase = pending.resume === 'battle' ? 'battleAssign' : 'actions';
  const { declare } = applyVox(s, a);

  if (declare) {
    declareAmbition(s, v, declare, player, s.turn?.mode ?? 'lead');
    // Farseers triggers on any declaration, including this one.
    if (offerPeek(s, player, resume)) return;
  }

  s.phase = resume;
  if (resume === 'battleAssign') settleBattle(s, v);
  else afterAction(s, v);
}

/**
 * Prelude resource actions (p17). `as` is set when a Loyal Guild card lets the
 * token be spent as a different type: the token returned to the supply is the
 * real one, but the action taken is the one printed for `as`.
 */
function spendResource(s: GameState, slot: number, as?: ResourceType): void {
  const turn = s.turn!;
  const p = s.playerStates[turn.player];
  const real = p.resources[slot];
  if (!real) throw new Error('no resource in that slot');
  const r = as ?? real;
  p.resources[slot] = null;
  turn.preludeSpent.push(real);

  switch (r) {
    case 'material':
      turn.freeActions.push(['build', 'repair']);
      break;
    case 'fuel':
      turn.freeActions.push(['move']);
      break;
    case 'relic':
      turn.freeActions.push(['secure']);
      break;
    case 'psionic':
      turn.freeActions.push([...SUIT_ACTIONS[actionCard(s.round.lead!.card).suit]]);
      break;
    case 'weapon':
      // "This turn, you may spend any action pips from your card play to take
      // Battle actions." — it modifies the card's pips, not a free action.
      turn.weaponSpent = true;
      break;
  }
}

/** Spent Prelude resources return to the supply when the Prelude ends (p20). */
function endPrelude(s: GameState): void {
  const turn = s.turn!;
  if (!turn.preludeOver) {
    for (const r of turn.preludeSpent) returnToSupply(s, r);
    turn.preludeSpent = [];
    turn.preludeOver = true;
  }
  s.phase = 'actions';
}

/**
 * Pay for one action. Free grants from resources are consumed before card
 * pips, most-restrictive first (engine ruling — see the file header).
 */
function payFor(s: GameState, kind: ActionKind): void {
  const turn = s.turn!;
  let best = -1;
  for (let i = 0; i < turn.freeActions.length; i++) {
    const grant = turn.freeActions[i];
    if (!grant.includes(kind)) continue;
    if (best < 0 || grant.length < turn.freeActions[best].length) best = i;
  }
  if (best >= 0) {
    turn.freeActions.splice(best, 1);
    return;
  }
  if (turn.pipsLeft > 0 && (turn.pipActions.includes(kind) || (turn.weaponSpent && kind === 'battle'))) {
    turn.pipsLeft--;
    return;
  }
  throw new Error(`cannot pay for ${kind}`);
}

// --- standard actions -----------------------------------------------------

function performAction(s: GameState, v: VariantDef, a: Action): void {
  endPrelude(s);
  const player = s.turn!.player;

  switch (a.t) {
    case 'tax': {
      payFor(s, 'tax');
      const building = s.systems[a.system].buildings[a.building];
      building.taxedThisTurn = true;
      const type = v.systems[a.system].planetType;
      if (type) takeFromSupply(s, player, type);
      if (building.player !== player) captureAgent(s, player, building.player);
      break;
    }
    case 'buildShip': {
      payFor(s, 'build');
      const p = s.playerStates[player];
      const st = s.systems[a.system];
      st.buildings[a.building].builtThisTurn = true;
      p.shipsSupply--;
      const controller = controlOf(s, a.system);
      if (controller !== null && controller !== player) st.damaged[player]++;
      else st.fresh[player]++;
      break;
    }
    case 'buildBuilding': {
      payFor(s, 'build');
      const p = s.playerStates[player];
      const controller = controlOf(s, a.system);
      const damaged = controller !== null && controller !== player;
      if (a.kind === 'city') p.citiesUsed++;
      else p.starportsSupply--;
      s.systems[a.system].buildings.push({
        player,
        kind: a.kind,
        damaged,
        taxedThisTurn: false,
        builtThisTurn: false,
      });
      if (a.kind === 'city') compactResources(p);
      break;
    }
    case 'move': {
      payFor(s, 'move');
      moveShipsInto(s, v, a.from, a.to, player, a.ships);
      return; // may open a catapult decision
    }
    case 'repair': {
      payFor(s, 'repair');
      const st = s.systems[a.system];
      if (a.building === null) {
        st.damaged[player]--;
        st.fresh[player]++;
      } else {
        st.buildings[a.building].damaged = false;
      }
      break;
    }
    case 'influence': {
      payFor(s, 'influence');
      s.playerStates[player].agentsSupply--;
      s.court[a.slot].agents[player]++;
      break;
    }
    case 'secure': {
      payFor(s, 'secure');
      secureCard(s, v, player, a.slot, false);
      break;
    }
    case 'cardAction': {
      payFor(s, cardActionCost(a.card, a.name));
      applyCardAction(s, player, a);
      break;
    }
    case 'battle': {
      payFor(s, 'battle');
      s.battle = {
        rolled: { assault: [], skirmish: [], raid: [] },
        system: a.system,
        attacker: player,
        defender: a.defender,
        dice: { assault: a.assault, skirmish: a.skirmish, raid: a.raid },
        selfHits: 0,
        intercept: 0,
        hits: 0,
        buildingHits: 0,
        keys: 0,
        interceptResolved: false,
        skirmishBlanks: 0,
        pendingReroll: 0,
        rerollDone: false,
      };
      s.stats.battles++;
      s.phase = 'battleRoll';
      return;
    }
    default:
      throw new Error(`unhandled action ${(a as Action).t}`);
  }
  afterAction(s, v);
}

/**
 * Move ships, then offer Catapult moves when they left a Loyal starport and
 * landed on a gate nobody else controls (p13).
 *
 * "When you take a Catapult move, check for control before moving in, not
 * after" (p13) — so the destination's controller is read before the ships
 * arrive, otherwise the arriving ships would clear the blockade themselves.
 */
function moveShipsInto(
  s: GameState,
  v: VariantDef,
  from: number,
  to: number,
  player: number,
  ships: number,
): void {
  const catapulting = s.phase === 'catapult';
  const visited = catapulting ? [...s.move!.visited] : [from];
  const controllerBefore = controlOf(s, to);

  const src = s.systems[from];
  const dst = s.systems[to];
  // Move fresh ships first, then damaged ones.
  let left = ships;
  const fresh = Math.min(src.fresh[player], left);
  src.fresh[player] -= fresh;
  dst.fresh[player] += fresh;
  left -= fresh;
  const damaged = Math.min(src.damaged[player], left);
  src.damaged[player] -= damaged;
  dst.damaged[player] += damaged;

  // Loyal starport only — "you can't Catapult from Rival starports you control".
  const canCatapult = catapulting || hasBuilding(s, from, player, 'starport');
  // "keep moving ... until they move to a gate controlled by anyone else or
  // they move to any planet"
  const blocked =
    !isGate(to) || (controllerBefore !== null && controllerBefore !== player);

  visited.push(to);
  const onward = neighbours(v, s, to).some((n) => !visited.includes(n));
  if (canCatapult && !blocked && ships > 0 && onward) {
    s.move = { at: to, ships, visited };
    s.phase = 'catapult';
    return;
  }
  s.move = null;
  s.phase = 'actions';
  afterAction(s, v);
}

/** Take a Court card, capture the Rival agents on it, refill the slot (p13). */
function secureCard(
  s: GameState,
  v: VariantDef,
  player: number,
  slot: number,
  ransack: boolean,
): void {
  const courtSlot = s.court[slot];
  const card = courtCard(courtSlot.card);

  s.playerStates[player].agentsSupply += courtSlot.agents[player];
  for (let p = 0; p < s.players; p++) {
    if (p === player) continue;
    for (let i = 0; i < courtSlot.agents[p]; i++) {
      // Ransacking takes agents as Trophies, not Captives (p16).
      if (ransack) s.playerStates[player].trophies.push({ owner: p, kind: 'ship' });
      else s.playerStates[player].captives.push(p);
    }
    courtSlot.agents[p] = 0;
  }
  courtSlot.agents[player] = 0;

  if (card.kind === 'vox' || card.discardOnSecure) {
    // A Vox card resolves When Secured. The ones that need a decision overlay
    // `pendingVox` on the current phase; `afterAction` and `settleBattle` stall
    // until it is answered, then resume where they left off.
    if (voxNeedsDecision(courtSlot.card)) {
      s.pendingVox = {
        card: courtSlot.card,
        player,
        resume: s.phase === 'battleAssign' ? 'battle' : 'actions',
      };
    } else {
      applyVoxImmediate(s, player, courtSlot.card);
    }
  } else {
    gainGuildCard(s, player, courtSlot.card);
    if (s.turn) s.turn.securedThisPrelude.push(courtSlot.card);
  }

  // Securing is a decision, not a chance node — the Court deck was shuffled at
  // setup and is drawn in order. If it ever empties, the discard comes back in
  // reverse, which keeps the refill a deterministic function of the state
  // rather than smuggling in an RNG the caller did not provide.
  if (s.courtDeck.length === 0) s.courtDeck = s.courtDiscard.splice(0).reverse();
  const next = s.courtDeck.pop();
  if (next !== undefined) {
    s.court[slot] = { card: next, agents: Array(s.players).fill(0) };
  } else {
    s.court.splice(slot, 1);
  }
}

function captureAgent(s: GameState, captor: number, victim: number): void {
  const v = s.playerStates[victim];
  if (v.agentsSupply <= 0) return;
  v.agentsSupply--;
  s.playerStates[captor].captives.push(victim);
}

// --- battle ---------------------------------------------------------------

function battleAssign(s: GameState, v: VariantDef, a: Action): void {
  const b = s.battle!;
  const st = s.systems[b.system];

  switch (a.t) {
    case 'assignSelf': {
      b.selfHits--;
      hitShip(s, b.system, b.attacker, a.fresh, b.defender);
      break;
    }
    case 'assignHit': {
      if (a.target === 'ship') {
        b.hits--;
        hitShip(s, b.system, b.defender, a.fresh, b.attacker);
      } else {
        // Hits spill onto buildings only once no defending ships remain.
        if (b.hits > 0 && st.fresh[b.defender] + st.damaged[b.defender] === 0) b.hits--;
        else b.buildingHits--;
        hitBuilding(s, v, b.system, a.building, b.attacker);
      }
      break;
    }
    case 'raidResource': {
      const victim = s.playerStates[b.defender];
      const type = victim.resources[a.slot]!;
      b.keys -= raidCost(a.slot);
      victim.resources[a.slot] = null;
      if (!stealResource(s, b.attacker, type)) returnToSupply(s, type);
      break;
    }
    case 'raidCard': {
      b.keys -= courtCard(a.card).raidCost;
      stealGuildCard(s, b.defender, b.attacker, a.card);
      break;
    }
    case 'raidDone': {
      b.keys = 0;
      break;
    }
    default:
      throw new Error(`unhandled battle action ${(a as Action).t}`);
  }
  settleBattle(s, v);
}

/** Steal a resource straight into the attacker's slots (no supply round-trip). */
function stealResource(s: GameState, player: number, type: ResourceType): boolean {
  const p = s.playerStates[player];
  const open = openResourceSlots(p);
  for (let i = 0; i < open; i++) {
    if (p.resources[i] === null) {
      p.resources[i] = type;
      return true;
    }
  }
  return false;
}

/** Damage a fresh ship, or destroy a damaged one into `taker`'s Trophies. */
function hitShip(
  s: GameState,
  system: number,
  owner: number,
  fresh: boolean,
  taker: number,
): void {
  const st = s.systems[system];
  if (fresh) {
    st.fresh[owner]--;
    st.damaged[owner]++;
    return;
  }
  st.damaged[owner]--;
  s.playerStates[taker].trophies.push({ owner, kind: 'ship' });
}

function hitBuilding(
  s: GameState,
  v: VariantDef,
  system: number,
  index: number,
  taker: number,
): void {
  const st = s.systems[system];
  const b = st.buildings[index];
  if (!b) return;
  if (!b.damaged) {
    b.damaged = true;
    return;
  }
  st.buildings.splice(index, 1);
  s.playerStates[taker].trophies.push({ owner: b.player, kind: b.kind });
  if (b.kind === 'city') destroyCity(s, v, taker, b.player, system);
}

/** Provoke Outrage, then Ransack the Court (p16). */
function destroyCity(
  s: GameState,
  v: VariantDef,
  destroyer: number,
  victim: number,
  system: number,
): void {
  const type = v.systems[system].planetType;
  if (type) provokeOutrage(s, destroyer, type);

  // Ransack: secure a card holding any of the victim's agents. Engine ruling —
  // pick the card with the most of them, ties leftmost.
  let target = -1;
  let most = 0;
  s.court.forEach((slot, i) => {
    if (slot.agents[victim] > most) {
      most = slot.agents[victim];
      target = i;
    }
  });
  if (target >= 0) secureCard(s, v, destroyer, target, true);
}

/** Advance or finish the battle once a hit has been assigned. */
function settleBattle(s: GameState, v: VariantDef): void {
  // A Ransack can secure a Vox card mid-battle; answer it before continuing.
  if (s.pendingVox) return;
  const b = s.battle;
  if (!b) return;
  const st = s.systems[b.system];

  // Drop hits that have nothing left to land on.
  if (b.selfHits > 0 && st.fresh[b.attacker] + st.damaged[b.attacker] === 0) b.selfHits = 0;
  const defenderShips = st.fresh[b.defender] + st.damaged[b.defender];
  const defenderBuildings = st.buildings.some((x) => x.player === b.defender);
  if (b.hits > 0 && defenderShips === 0 && !defenderBuildings) b.hits = 0;
  if (b.buildingHits > 0 && !defenderBuildings) b.buildingHits = 0;
  if (b.keys > 0) {
    const victim = s.playerStates[b.defender];
    const shield = theftImmune(victim) ? theftImmunityCard(victim) : null;
    const canSteal =
      shield !== null
        ? courtCard(shield).raidCost <= b.keys
        : victim.resources.some((r, i) => r !== null && raidCost(i) <= b.keys) ||
          victim.guildCards.some((c) => courtCard(c).raidCost <= b.keys);
    if (!canSteal || shipsOf(s, b.system, b.attacker) === 0) b.keys = 0;
  }

  if (b.selfHits === 0 && b.hits === 0 && b.buildingHits === 0 && b.keys === 0) {
    s.battle = null;
    s.phase = 'actions';
    afterAction(s, v);
    return;
  }
  s.phase = 'battleAssign';
}

// --- turn / round / chapter flow -----------------------------------------

/** After any action: if nothing can still be spent, the turn ends itself. */
function afterAction(s: GameState, v: VariantDef): void {
  // A secured Vox card is answered before the turn can advance or end.
  if (s.pendingVox) return;
  const turn = s.turn;
  if (!turn) return;
  if (turn.pipsLeft === 0 && turn.freeActions.length === 0) {
    endTurn(s, v);
  }
}

function endTurn(s: GameState, v: VariantDef): void {
  endPrelude(s);
  const player = s.turn!.player;

  // "Rarely, a player will have no starports or ships on the map ... they place
  // 3 fresh ships in any gate at the end of their turn." (p22)
  if (isWipedOut(s, player) && s.playerStates[player].shipsSupply > 0) {
    s.reinforcing = player;
    s.phase = 'reinforce';
    return;
  }
  finishTurn(s, v);
}

function finishTurn(s: GameState, v: VariantDef): void {
  // Per-turn flags reset (tax limit p12, starport build limit p12).
  for (const st of s.systems) {
    for (const b of st.buildings) {
      b.taxedThisTurn = false;
      b.builtThisTurn = false;
    }
  }
  s.turn = null;
  s.battle = null;
  s.move = null;
  s.round.turnIndex++;
  s.phase = 'play';
  ensureActor(s, v);
}

function beginRound(s: GameState, v: VariantDef): void {
  s.round.turnOrder = Array.from({ length: v.players }, (_, i) => (s.initiative + i) % v.players);
  s.round.turnIndex = 0;
  s.round.lead = null;
  s.round.leadNumber = 0;
  s.round.played = [];
  s.round.seizedBy = null;
  s.round.ambitionDeclared = false;
  s.initiativeSeized = false;
  s.turn = null;
  s.phase = 'play';
  ensureActor(s, v);
}

/** Skip card-less players; a card-less initiative holder must pass (p8). */
function ensureActor(s: GameState, v: VariantDef): void {
  for (;;) {
    if (s.round.turnIndex >= s.round.turnOrder.length) {
      endRound(s, v);
      return;
    }
    const player = s.round.turnOrder[s.round.turnIndex];
    if (s.playerStates[player].hand.length > 0) return;
    if (s.round.turnIndex === 0) {
      passInitiative(s, v);
      return;
    }
    s.round.turnIndex++;
  }
}

/**
 * "Give the initiative marker to the next clockwise player who has any cards in
 * their hand, then immediately end the round." (p8)
 */
function passInitiative(s: GameState, v: VariantDef): void {
  const from = s.round.turnOrder[s.round.turnIndex] ?? s.initiative;
  const withCards = s.playerStates.filter((p) => p.hand.length > 0).length;
  s.round.consecutivePasses++;

  let next: number | null = null;
  for (let k = 1; k <= v.players; k++) {
    const cand = (from + k) % v.players;
    if (s.playerStates[cand].hand.length > 0) {
      next = cand;
      break;
    }
  }
  if (next === null || withCards === 0) {
    endChapter(s, v);
    return;
  }
  s.initiative = next;
  discardPlayed(s);
  s.stats.rounds++;

  // "If everyone with cards in their hand passes consecutively ... end the
  // chapter." (p8)
  if (s.round.consecutivePasses >= withCards) {
    endChapter(s, v);
    return;
  }
  beginRound(s, v);
}

/**
 * Discard the round's played cards — but first hand out any card a Union is
 * attached to: "When the round ends, draw that card into your hand and discard
 * this card." (p20)
 */
function discardPlayed(s: GameState): void {
  const claimed = new Map<number, number>(); // action card -> claiming player
  for (const u of s.unions) {
    if (s.round.played.some((c) => c.card === u.target) && !claimed.has(u.target)) {
      claimed.set(u.target, u.player);
    }
    discardGuildCard(s, u.player, u.card);
  }
  s.unions = [];

  for (const c of s.round.played) {
    const claimer = claimed.get(c.card);
    if (claimer !== undefined) {
      // A Union pulls the card back into a hand. Everyone saw it, but it is no
      // longer out of play, so it does not join `revealed` — the conservative
      // choice loses information rather than inventing an impossible world.
      s.playerStates[claimer].hand.push(c.card);
    } else {
      s.actionDiscard.push(c.card);
      // Played face up: the whole table watched this card leave play.
      if (!c.faceDown) s.revealed.push(c.card);
    }
  }
  s.round.played = [];
}

/** Steps 3 and 4 of a round: check initiative, discard, next round (p11). */
function endRound(s: GameState, v: VariantDef): void {
  if (s.round.seizedBy !== null) {
    s.initiative = s.round.seizedBy;
  } else {
    let best = -1;
    let winner: number | null = null;
    for (const c of s.round.played) {
      if (c.mode !== 'surpass') continue;
      const n = actionCard(c.card).number;
      if (n > best) {
        best = n;
        winner = c.player;
      }
    }
    if (winner !== null) s.initiative = winner;
  }

  discardPlayed(s);
  s.stats.rounds++;

  if (s.playerStates.some((p) => p.hand.length > 0)) beginRound(s, v);
  else endChapter(s, v);
}

/** Ending a chapter: score, clean up, flip, end or advance (p18-p19). */
function endChapter(s: GameState, v: VariantDef): void {
  discardPlayed(s);
  for (const p of s.playerStates) {
    s.actionDiscard.push(...p.hand);
    p.hand = [];
  }

  const { results, gained } = scoreChapter(s, v);
  gained.forEach((g, i) => (s.playerStates[i].power += g));

  const scored = new Set(results.map((r) => r.ambition));
  if (scored.has('warlord')) returnTrophies(s);
  if (scored.has('tyrant')) returnCaptives(s);
  cartelSqueeze(s);

  for (const a of AMBITIONS) {
    s.availableMarkers.push(...s.declared[a]);
    s.declared[a] = [];
  }
  s.availableMarkers.sort((a, b) => a - b);
  flipLowestUnflipped(s, v);
  s.stats.chapters++;

  const done =
    s.playerStates.some((p) => p.power >= v.powerThreshold) || s.chapter >= v.maxChapters;
  if (done) {
    s.phase = 'over';
    return;
  }
  s.chapter++;
  s.phase = 'deal';
}

/**
 * "After scoring, Rivals discard all Material / Fuel" — the two Cartel cards
 * (p20). The tokens go onto the Cartel card, which is where that type's supply
 * lives while the card is in play.
 */
function cartelSqueeze(s: GameState): void {
  for (const type of RESOURCE_TYPES) {
    const holder = cartelHolder(s, type);
    if (holder === null) continue;
    for (let p = 0; p < s.players; p++) {
      if (p === holder) continue;
      const ps = s.playerStates[p];
      for (let i = 0; i < ps.resources.length; i++) {
        if (ps.resources[i] === type) {
          ps.resources[i] = null;
          s.cartel[type]++;
        }
      }
    }
  }
}

/** Trophies go home: pieces to their supplies, cities back onto their board. */
function returnTrophies(s: GameState): void {
  for (const p of s.playerStates) {
    for (const t of p.trophies) {
      const owner = s.playerStates[t.owner];
      if (t.kind === 'ship') owner.shipsSupply++;
      else if (t.kind === 'starport') owner.starportsSupply++;
      else if (t.kind === 'agent') owner.agentsSupply++;
      else {
        owner.citiesUsed = Math.max(0, owner.citiesUsed - 1);
        compactResources(owner);
      }
    }
    p.trophies = [];
  }
}

function returnCaptives(s: GameState): void {
  for (const p of s.playerStates) {
    for (const owner of p.captives) s.playerStates[owner].agentsSupply++;
    p.captives = [];
  }
}

// ---------------------------------------------------------------------------
// Results
// ---------------------------------------------------------------------------

export interface Standing {
  player: number;
  power: number;
  rank: number;
}

/**
 * Final standings. "On a tie, the tied player earliest in turn order is the
 * winner" (p19) — turn order runs from the initiative holder clockwise.
 */
export function standings(s: GameState): Standing[] {
  const order = Array.from({ length: s.players }, (_, i) => (s.initiative + i) % s.players);
  const sorted = [...order].sort(
    (a, b) => s.playerStates[b].power - s.playerStates[a].power || order.indexOf(a) - order.indexOf(b),
  );
  return sorted.map((player, i) => ({ player, power: s.playerStates[player].power, rank: i }));
}

export function winner(s: GameState): number {
  return standings(s)[0].player;
}

/** Resources a player could spend right now, for bots and the UI. */
export function spendable(s: GameState, player: number): ResourceType[] {
  const p = s.playerStates[player];
  return heldResources(p).filter((r) => !p.outrage[r]);
}

export { controlOf, DICE_PER_TYPE };
export type { DieType };
