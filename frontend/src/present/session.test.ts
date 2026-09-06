import { describe, expect, it } from 'vitest';
import {
  advance, audienceSlide, codeLife, CODE_TTL_MS, isValidCode, jump, nextSlide, normaliseCode, pairingCode,
  setBlank, stageSlide, withMessage, withStageMessage, DEFAULT_THEME, type SessionState,
} from './session';
import type { Slide } from './slides';

function slide(id: string): Slide {
  return {
    id, itemId: 'item', kind: 'lyrics', songTitle: 'Song', label: null, lines: [],
    text: null, sheetId: null, page: null, writtenKey: 'G', setKey: null, capo: 0,
  };
}

const state: SessionState = {
  session_id: 'session', workspace_id: 'workspace',
  set_snapshot: { setId: 'set', setName: 'Sunday', items: [], takenAt: '2026-09-06T09:00:00Z' },
  slides: [slide('a'), slide('b'), slide('c')],
  index: 0, blank_mode: 'none', message: null, stage_message: null,
  theme: DEFAULT_THEME, started_at: '2026-09-06T09:00:00Z', revision: 1, ended: false,
};

describe('session state', () => {
  it('moves and bumps the revision, which is what outputs order themselves by', () => {
    const next = advance(state, 1);

    expect(next.index).toBe(1);
    expect(next.revision).toBe(2);
  });

  it('stops at both ends without bumping the revision for a move that does nothing', () => {
    expect(advance(state, -1)).toBe(state);
    const atEnd = { ...state, index: 2 };
    expect(advance(atEnd, 1)).toBe(atEnd);
    expect(advance(state, 99).index).toBe(2);
  });

  it('jumps to a slide, clamped to the list', () => {
    expect(jump(state, 2).index).toBe(2);
    expect(jump(state, 99).index).toBe(2);
    expect(jump(state, -5).index).toBe(0);
  });

  // Acceptance criterion 4: blanking the audience must not blank the stage.
  it('blanks the audience and leaves the stage showing the words', () => {
    const blanked = setBlank(state, 'black');

    expect(audienceSlide(blanked)).toBeNull();
    expect(stageSlide(blanked)?.id).toBe('a');
    expect(nextSlide(blanked)?.id).toBe('b');
  });

  it('toggles the same blank mode off again', () => {
    expect(setBlank(setBlank(state, 'logo'), 'logo').blank_mode).toBe('none');
  });

  it('freezing leaves the audience on the slide it is on', () => {
    expect(audienceSlide(setBlank(state, 'freeze'))?.id).toBe('a');
  });

  it('keeps audience and stage messages apart', () => {
    const withBoth = withStageMessage(withMessage(state, 'Starting soon'), 'two more times');

    expect(withBoth.message).toBe('Starting soon');
    expect(withBoth.stage_message).toBe('two more times');
    expect(withMessage(withBoth, '').message).toBeNull();
  });
});

describe('pairing codes', () => {
  it('avoids characters that are misread on a dark stage', () => {
    for (let attempt = 0; attempt < 200; attempt++) {
      const code = pairingCode();

      expect(code).toHaveLength(6);
      expect(code).not.toMatch(/[01OI]/);
      expect(isValidCode(code)).toBe(true);
    }
  });

  it('runs out after half an hour, and says how long is left until it does', () => {
    const issued = Date.parse('2026-09-06T10:00:00Z');
    const expires = issued + CODE_TTL_MS;

    expect(codeLife(expires, issued).expired).toBe(false);
    expect(codeLife(expires, issued).minutesLeft).toBe(30);
    expect(codeLife(expires, issued + 29.5 * 60_000).minutesLeft).toBe(1);
    expect(codeLife(expires, expires - 1).expired).toBe(false);
    expect(codeLife(expires, expires).expired).toBe(true);
    expect(codeLife(expires, expires + 60_000).expired).toBe(true);
  });

  it('accepts what a person types, in any case, with spaces', () => {
    expect(normaliseCode(' 4kj9qp ')).toBe('4KJ9QP');
    expect(isValidCode('4kj-9qp')).toBe(true);
    expect(isValidCode('4KJ9Q')).toBe(false);
    expect(isValidCode('4KJ9Q0')).toBe(false);
  });
});
