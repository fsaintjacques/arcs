/**
 * The bridge between the React UI and the Rust engine compiled to wasm.
 *
 * The UI used to import the TypeScript engine directly. It now drives one
 * `Session` in `rust/crates/arcs-wasm`: ask what the game needs, read the
 * legal list, **choose by index**, resolve chance, snapshot and restore. The
 * TS engine stays in the repo as the reference implementation — the tests, the
 * simulator and the CLI still run on it — but nothing on this path does.
 *
 * The Rust side emits JSON in the shapes `src/engine/types.ts` declares, so
 * every component below `useGame` keeps rendering `GameState`, `VariantDef`
 * and `Action` exactly as before. This module owns the two seams where that is
 * not quite literal:
 *
 *   - `round.lead` is re-pointed at the matching entry of `round.played`, so
 *     the identity comparison in `Trick` survives the round trip through JSON;
 *   - printed Court card text is filled in from the TS card table, which is
 *     where it lives — the Rust port carries card *rules*, not card *prose*.
 */
import type { Action, CourtCardDef, GameState, VariantDef } from '../engine/types';
import { courtCard } from '../engine/court';
import init, { Session, agentNames as wasmAgentNames } from './wasm/arcs_wasm.js';

export type { Session };

let ready: Promise<void> | null = null;

/**
 * Fetch and instantiate the wasm module. Idempotent, and awaited once before
 * the app renders (see `main.tsx`) so every hook can build sessions
 * synchronously afterwards.
 */
export function initEngine(): Promise<void> {
  ready ??= init().then(() => undefined);
  return ready;
}

/** The agents the Rust registry can put in a seat. */
export function agentNames(): string[] {
  return wasmAgentNames();
}

export interface SessionSpec {
  players: number;
  seed: number;
  setupIndex: number;
  /** Agent name per seat; `null` means a human plays it. */
  seats: (string | null)[];
}

export function createSession(spec: SessionSpec): Session {
  const session = new Session(spec.players, spec.setupIndex, spec.seed, 'deck');
  spec.seats.slice(0, spec.players).forEach((name, seat) => {
    session.setAgent(seat, name ?? undefined);
  });
  return session;
}

/** What the game needs next. */
export type Pending =
  | { kind: 'over' }
  | { kind: 'chance' }
  | { kind: 'decision'; player: number; nActions: number };

export function pending(s: Session): Pending {
  return JSON.parse(s.pending()) as Pending;
}

export function readState(s: Session): GameState {
  const state = JSON.parse(s.stateJson()) as GameState;
  // JSON has no aliasing: `round.lead` came back as a copy of the played card
  // it *is* on the Rust side. `Trick` tells the lead apart by identity, so the
  // alias is restored here rather than by rewriting the component.
  const lead = state.round.lead;
  if (lead) {
    const same = state.round.played.find(
      (c) => c.player === lead.player && c.card === lead.card && c.mode === lead.mode,
    );
    if (same) state.round.lead = same;
  }
  return state;
}

export function readVariant(s: Session): VariantDef {
  const variant = JSON.parse(s.variantJson()) as VariantDef;
  variant.courtDeck = variant.courtDeck.map(
    (c): CourtCardDef => ({ ...c, text: courtCard(c.id).text }),
  );
  return variant;
}

/** The legal actions, in the order `apply(index)` expects. */
export function readActions(s: Session): Action[] {
  return JSON.parse(s.legal()) as Action[];
}

export function readStandings(s: Session): { player: number; power: number; rank: number }[] {
  return JSON.parse(s.standings()) as { player: number; power: number; rank: number }[];
}

/** Ambition tallies per seat, in `AMBITIONS` order. */
export function readAmbitionCounts(s: Session): number[][] {
  return JSON.parse(s.ambitionCounts()) as number[][];
}
