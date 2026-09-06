import type { Slide, Snapshot } from './slides';

/**
 * The one object every screen in a session renders from.
 *
 * There is exactly one writer — the control surface — and every output is a pure function of
 * what it last received. That is what lets an audience window be reloaded mid-service and come
 * back on the right slide without asking anyone anything.
 */

export type BlankMode = 'none' | 'black' | 'logo' | 'freeze';

export interface Theme {
  id: string;
  name: string;
  font_family: string;
  /** Maximum size; the fitting pass may come down from it. */
  font_size_vh: number;
  text_color: string;
  background_kind: 'color' | 'gradient' | 'image';
  background_value: string;
  align: 'left' | 'center';
  safe_area_pct: number;
  show_section_labels: boolean;
}

export const DEFAULT_THEME: Theme = {
  id: 'default',
  name: 'Default',
  font_family: 'system-ui, sans-serif',
  font_size_vh: 8,
  text_color: '#ffffff',
  background_kind: 'color',
  background_value: '#000000',
  align: 'center',
  safe_area_pct: 5,
  show_section_labels: false,
};

export interface SessionState {
  session_id: string;
  workspace_id: string;
  set_snapshot: Snapshot;
  slides: Slide[];
  index: number;
  blank_mode: BlankMode;
  /** An overlay for the audience only — "the service will begin in five minutes". */
  message: string | null;
  /** A note for the stage views only — "two more times", "wrap it up". */
  stage_message: string | null;
  theme: Theme;
  started_at: string;
  /** Monotonic. An output ignores anything it receives with a lower revision. */
  revision: number;
  ended: boolean;
}

export type OutputKind = 'audience' | 'stage' | 'paired-stage';

export interface OutputStatus {
  output_id: string;
  kind: OutputKind;
  label: string;
  joined_at: string;
  last_ack_revision: number;
  responding: boolean;
  can_advance: boolean;
}

/** Messages on the wire, whichever wire it is. */
export type SessionMessage =
  | { type: 'state'; state: SessionState }
  | { type: 'ack'; output_id: string; kind: OutputKind; label: string; revision: number }
  | { type: 'hello'; output_id: string; kind: OutputKind; label: string }
  | { type: 'request-state'; output_id: string }
  | { type: 'advance'; output_id: string; delta: number }
  | { type: 'bye'; output_id: string };

export function channelName(sessionId: string): string {
  return `aurum-session-${sessionId}`;
}

export function advance(state: SessionState, delta: number): SessionState {
  const index = Math.min(Math.max(state.index + delta, 0), Math.max(state.slides.length - 1, 0));

  return index === state.index ? state : { ...state, index, revision: state.revision + 1 };
}

export function jump(state: SessionState, index: number): SessionState {
  const bounded = Math.min(Math.max(index, 0), Math.max(state.slides.length - 1, 0));

  return { ...state, index: bounded, revision: state.revision + 1 };
}

export function setBlank(state: SessionState, mode: BlankMode): SessionState {
  return { ...state, blank_mode: state.blank_mode === mode ? 'none' : mode, revision: state.revision + 1 };
}

export function withMessage(state: SessionState, message: string | null): SessionState {
  return { ...state, message: message === '' ? null : message, revision: state.revision + 1 };
}

export function withStageMessage(state: SessionState, message: string | null): SessionState {
  return { ...state, stage_message: message === '' ? null : message, revision: state.revision + 1 };
}

/** The slide the audience should be showing, which is not always the current one. */
export function audienceSlide(state: SessionState): Slide | null {
  if (state.blank_mode === 'black' || state.blank_mode === 'logo') {
    return null;
  }

  return state.slides[state.index] ?? null;
}

/**
 * The slide the stage should be showing. Blanking the audience never blanks the stage — the
 * band still needs the words while the congregation looks at a logo (acceptance criterion 4).
 */
export function stageSlide(state: SessionState): Slide | null {
  return state.slides[state.index] ?? null;
}

export function nextSlide(state: SessionState): Slide | null {
  return state.slides[state.index + 1] ?? null;
}

/** Codes avoid characters that are misread on a dark stage: no 0/O, no 1/I. */
const ALPHABET = 'ABCDEFGHJKLMNPQRSTUVWXYZ23456789';

export function pairingCode(): string {
  const bytes = crypto.getRandomValues(new Uint8Array(6));

  return [...bytes].map((byte) => ALPHABET[byte % ALPHABET.length]!).join('');
}

export const CODE_TTL_MS = 30 * 60 * 1000;

export function normaliseCode(input: string): string {
  return input.trim().toUpperCase().replace(/[^A-Z2-9]/g, '');
}

export function isValidCode(input: string): boolean {
  const code = normaliseCode(input);

  return code.length === 6 && [...code].every((character) => ALPHABET.includes(character));
}
