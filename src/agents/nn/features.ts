/**
 * GameState → fixed-length Float32Array, from one player's perspective.
 *
 * Everything is seat-rotated (index 0 is always "me", rivals follow in turn
 * order) and roughly [0,1]-scaled, so one trained net serves every seat and
 * player count; absent seats and out-of-play systems read as zeros. The
 * layout is versioned by FEATURE_SIZE — change the layout, retrain the net.
 *
 * Known gap, accepted for v1: Court and Guild cards are encoded by counts
 * and suits, not identity, so the net cannot learn card-specific tactics.
 */
import { actionCard, ambitionCount, AMBITIONS, controlOf, courtCard, markerValue, openResourceSlots, SUITS } from '../../engine';
import type { GameState, ResourceType, VariantDef } from '../../engine';

const SYSTEMS = 24;
const MAX_SEATS = 4;
const PER_SYSTEM = 10;
const PER_AMBITION = 6;
const PER_SEAT = 12;
const HAND_EXTRAS = 6;
const COURT_SLOTS = 4;
const PER_COURT = 4;
const GLOBALS = 8;

export const FEATURE_SIZE =
  SYSTEMS * PER_SYSTEM +
  AMBITIONS.length * PER_AMBITION +
  MAX_SEATS * PER_SEAT +
  HAND_EXTRAS +
  COURT_SLOTS * PER_COURT +
  GLOBALS;

const RESOURCE_ORDER: ResourceType[] = ['material', 'fuel', 'weapon', 'relic', 'psionic'];

/**
 * Fill `out` (length FEATURE_SIZE, caller-owned) with the position as seen
 * by `player`. Zero-alloc on the hot path.
 */
export function extractFeatures(
  s: GameState,
  v: VariantDef,
  player: number,
  out: Float32Array,
): void {
  out.fill(0);
  let k = 0;

  // --- systems ---
  for (let i = 0; i < SYSTEMS; i++, k += PER_SYSTEM) {
    const st = s.systems[i];
    if (!st || st.outOfPlay) continue;
    let rivalMaxFresh = 0;
    let rivalShips = 0;
    for (let p = 0; p < s.players; p++) {
      if (p === player) continue;
      if (st.fresh[p] > rivalMaxFresh) rivalMaxFresh = st.fresh[p];
      rivalShips += st.fresh[p] + st.damaged[p];
    }
    let myCity = 0;
    let myStarport = 0;
    let rivalCities = 0;
    let rivalStarports = 0;
    for (const b of st.buildings) {
      if (b.player === player) {
        if (b.kind === 'city') myCity++;
        else myStarport++;
      } else if (b.kind === 'city') rivalCities++;
      else rivalStarports++;
    }
    const ctrl = controlOf(s, i);
    out[k] = st.fresh[player] / 4;
    out[k + 1] = st.damaged[player] / 4;
    out[k + 2] = myCity / 2;
    out[k + 3] = myStarport / 2;
    out[k + 4] = rivalMaxFresh / 4;
    out[k + 5] = rivalShips / 8;
    out[k + 6] = rivalCities / 2;
    out[k + 7] = rivalStarports / 2;
    out[k + 8] = ctrl === player ? 1 : 0;
    out[k + 9] = ctrl !== null && ctrl !== player ? 1 : 0;
  }

  // --- ambitions ---
  for (const ambition of AMBITIONS) {
    const mine = ambitionCount(s.playerStates[player], ambition);
    let bestRival = 0;
    for (let p = 0; p < s.players; p++) {
      if (p === player) continue;
      const c = ambitionCount(s.playerStates[p], ambition);
      if (c > bestRival) bestRival = c;
    }
    bestRival = Math.max(bestRival, s.phantom[ambition] ?? 0);
    let first = 0;
    let second = 0;
    for (const i of s.declared[ambition]) {
      const val = markerValue(v.ambitionMarkers, i, s.flipped[i]);
      first += val.first;
      second += val.second;
    }
    out[k] = mine / 6;
    out[k + 1] = bestRival / 6;
    out[k + 2] = first / 9;
    out[k + 3] = second / 6;
    out[k + 4] = s.declared[ambition].length / 3;
    out[k + 5] = mine > bestRival && mine > 0 ? 1 : 0;
    k += PER_AMBITION;
  }

  // --- seats, me first then rivals in turn order ---
  for (let seat = 0; seat < MAX_SEATS; seat++, k += PER_SEAT) {
    if (seat >= s.players) continue;
    const p = (player + seat) % s.players;
    const ps = s.playerStates[p];
    const open = openResourceSlots(ps);
    let agents = 0;
    for (const slot of s.court) agents += slot.agents[p] ?? 0;
    out[k] = ps.power / v.powerThreshold;
    out[k + 1] = ps.hand.length / 6;
    for (let r = 0; r < RESOURCE_ORDER.length; r++) {
      let n = 0;
      for (let i = 0; i < open; i++) if (ps.resources[i] === RESOURCE_ORDER[r]) n++;
      out[k + 2 + r] = n / 3;
    }
    out[k + 7] = ps.guildCards.length / 5;
    out[k + 8] = ps.captives.length / 8;
    out[k + 9] = ps.trophies.length / 8;
    out[k + 10] = agents / 10;
    out[k + 11] = s.initiative === p ? 1 : 0;
  }

  // --- my hand, beyond the size the seat block carries ---
  {
    const hand = s.playerStates[player].hand;
    let pips = 0;
    const suitCount = [0, 0, 0, 0];
    const topBySuit = [0, 0, 0, 0];
    for (const ps of s.playerStates) {
      for (const c of ps.hand) {
        const def = actionCard(c);
        const si = SUITS.indexOf(def.suit);
        if (def.number > topBySuit[si]) topBySuit[si] = def.number;
      }
    }
    let tops = 0;
    for (const c of hand) {
      const def = actionCard(c);
      pips += def.pips;
      const si = SUITS.indexOf(def.suit);
      suitCount[si]++;
      if (def.number === topBySuit[si]) tops++;
    }
    out[k] = pips / 24;
    out[k + 1] = tops / 4;
    for (let i = 0; i < 4; i++) out[k + 2 + i] = suitCount[i] / 6;
    k += HAND_EXTRAS;
  }

  // --- court row ---
  for (let slot = 0; slot < COURT_SLOTS; slot++, k += PER_COURT) {
    const c = s.court[slot];
    if (!c || c.card < 0) continue;
    let rivalMax = 0;
    for (let p = 0; p < s.players; p++) {
      if (p === player) continue;
      if (c.agents[p] > rivalMax) rivalMax = c.agents[p];
    }
    out[k] = (c.agents[player] ?? 0) / 6;
    out[k + 1] = rivalMax / 6;
    out[k + 2] = courtCard(c.card).kind === 'vox' ? 1 : 0;
    out[k + 3] = 1; // slot filled
  }

  // --- globals ---
  const chapter = Math.min(5, Math.max(1, s.chapter));
  out[k + (chapter - 1)] = 1; // chapter one-hot, 5 wide
  out[k + 5] = s.players / 4;
  out[k + 6] = s.actionDeck.length / 28;
  out[k + 7] = s.playerStates[player].hand.length > 0 ? 0 : 1; // out of cards
}
