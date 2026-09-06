import { useCallback, useState } from 'react';
import { DEFAULT_DISPLAY, type DisplayPrefs } from '../chart/ChartView';

/**
 * Display preferences are per user *and per device* — a 27-inch monitor and a phone on a mic
 * stand want different font sizes, and syncing that would make each device fight the others.
 * So these stay in localStorage and never enter the sync engine.
 */
const STORAGE_KEY = 'aurum.display';

export function loadDisplay(): DisplayPrefs {
  try {
    const stored = localStorage.getItem(STORAGE_KEY);

    return stored === null ? DEFAULT_DISPLAY : { ...DEFAULT_DISPLAY, ...(JSON.parse(stored) as Partial<DisplayPrefs>) };
  } catch {
    // A browser with storage blocked still gets a working chart, just not a remembered one.
    return DEFAULT_DISPLAY;
  }
}

export function saveDisplay(display: DisplayPrefs): void {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(display));
  } catch {
    // ignored — see above
  }
}

/** Display preferences as state, written through to storage on every change. */
export function useDisplay(): [DisplayPrefs, (next: DisplayPrefs) => void] {
  const [display, setDisplay] = useState(loadDisplay);

  const update = useCallback((next: DisplayPrefs) => {
    setDisplay(next);
    saveDisplay(next);
  }, []);

  return [display, update];
}
