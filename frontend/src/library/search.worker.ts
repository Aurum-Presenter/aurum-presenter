import { buildIndex, EMPTY_INDEX, search, type IndexedSong, type SearchHit, type SearchIndex } from './search';

/**
 * The search index lives in a worker so that rebuilding it — which happens on every song write
 * — never lands on the frame the user is typing into. The main thread only ever sends songs and
 * receives ranked ids.
 */

export type ToWorker =
  | { kind: 'build'; songs: IndexedSong[] }
  | { kind: 'query'; id: number; text: string };

export type FromWorker =
  | { kind: 'built'; songs: number }
  | { kind: 'results'; id: number; hits: SearchHit[] };

let index: SearchIndex = EMPTY_INDEX;

self.onmessage = (event: MessageEvent<ToWorker>): void => {
  const message = event.data;

  if (message.kind === 'build') {
    index = buildIndex(message.songs);
    (self as unknown as Worker).postMessage({ kind: 'built', songs: message.songs.length } satisfies FromWorker);
    return;
  }

  (self as unknown as Worker).postMessage({
    kind: 'results',
    id: message.id,
    hits: search(index, message.text),
  } satisfies FromWorker);
};
