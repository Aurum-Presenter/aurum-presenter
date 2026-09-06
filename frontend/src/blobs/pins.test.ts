import { describe, expect, it } from 'vitest';
import { isOutOfSpace } from './queue';
import { pinsFrom, type PinSources } from './pins';

const now = new Date('2026-09-06T12:00:00Z');

const sources: PinSources = {
  sets: [
    { id: 'coming-up', name: 'Sunday morning', pinned: 0, scheduled_for: '2026-09-08' },
    { id: 'kept', name: 'Carols', pinned: 1, scheduled_for: '2025-12-24' },
    { id: 'old', name: 'Last Easter', pinned: 0, scheduled_for: '2026-04-05' },
  ],
  items: [
    { set_id: 'coming-up', song_id: 'grace' },
    { set_id: 'kept', song_id: 'noel' },
    { set_id: 'old', song_id: 'grace' },
  ],
  sheets: [
    { id: 'grace-piano', song_id: 'grace' },
    { id: 'noel-satb', song_id: 'noel' },
    { id: 'thine-lead', song_id: 'thine' },
  ],
  songs: [
    { id: 'grace', title: 'Amazing Grace' },
    { id: 'noel', title: 'The First Noel' },
    { id: 'thine', title: 'Be Thou My Vision' },
  ],
  cached: [
    { sheet_id: 'grace-piano', size: 4 * 1024 * 1024 },
    { sheet_id: 'noel-satb', size: 12 * 1024 * 1024 },
    { sheet_id: 'thine-lead', size: 1 * 1024 * 1024 },
  ],
  preferences: [
    { scope_id: 'thine', value: JSON.stringify({ pinned: true }) },
    { scope_id: 'grace', value: JSON.stringify({ pinned: false }) },
    { scope_id: 'noel', value: 'not json' },
  ],
};

/**
 * Offline-storage acceptance criterion 6: a device that fills up names what is holding the room
 * rather than choosing for the user.
 */
describe('what a full device offers to release', () => {
  it('lists what is kept on purpose, biggest first', () => {
    const held = pinsFrom(sources, now);

    expect(held.map((pin) => `${pin.name} ${Math.round(pin.bytes / 1024 / 1024)}`)).toEqual([
      'Carols 12',
      'Sunday morning 4',
      'Be Thou My Vision 1',
    ]);
  });

  it('says why each of them is here, which is what the user is choosing between', () => {
    const held = pinsFrom(sources, now);

    expect(held.find((pin) => pin.name === 'Carols')?.reason).toBe('pinned');
    expect(held.find((pin) => pin.name === 'Sunday morning')?.reason).toBe('coming-up');
    expect(held.find((pin) => pin.name === 'Be Thou My Vision')?.reason).toBe('pinned');
  });

  it('leaves out a set that is neither pinned nor coming up', () => {
    expect(pinsFrom(sources, now).some((pin) => pin.name === 'Last Easter')).toBe(false);
  });

  it('ignores a preference it cannot read rather than failing', () => {
    expect(pinsFrom(sources, now).some((pin) => pin.name === 'The First Noel' && pin.kind === 'song')).toBe(false);
  });
});

describe('recognising a device with no room left', () => {
  it('knows the shapes browsers report it in', () => {
    expect(isOutOfSpace(Object.assign(new Error('x'), { name: 'QuotaExceededError' }))).toBe(true);
    expect(isOutOfSpace(Object.assign(new Error('x'), { name: 'NS_ERROR_FILE_NO_DEVICE_SPACE' }))).toBe(true);
    expect(isOutOfSpace(new Error('The quota has been exceeded.'))).toBe(true);
    expect(isOutOfSpace(new Error('No space left on device'))).toBe(true);
  });

  it('does not mistake an ordinary failure for a full disk', () => {
    expect(isOutOfSpace(new Error('Failed to fetch'))).toBe(false);
    expect(isOutOfSpace(null)).toBe(false);
  });
});
