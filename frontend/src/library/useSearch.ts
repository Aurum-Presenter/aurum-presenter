import { useEffect, useRef, useState } from 'react';
import { buildIndex, EMPTY_INDEX, search, type IndexedSong, type SearchHit, type SearchIndex } from './search';
import type { FromWorker, ToWorker } from './search.worker';

/**
 * Search wired to the worker, debounced the 250 ms the feature document asks for.
 *
 * If a Worker cannot be created — an old browser, a test runner — the same functions run on the
 * main thread instead. A library that is a little less smooth is better than a search box that
 * does nothing.
 */
export function useSearch(songs: IndexedSong[], query: string): SearchHit[] | null {
  const [hits, setHits] = useState<SearchHit[] | null>(null);
  const worker = useRef<Worker | null>(null);
  const fallback = useRef<SearchIndex>(EMPTY_INDEX);
  const nextId = useRef(0);

  useEffect(() => {
    try {
      worker.current = new Worker(new URL('./search.worker.ts', import.meta.url), { type: 'module' });
    } catch {
      worker.current = null;
    }

    return () => {
      worker.current?.terminate();
      worker.current = null;
    };
  }, []);

  useEffect(() => {
    const timer = setTimeout(() => {
      if (worker.current !== null) {
        worker.current.postMessage({ kind: 'build', songs } satisfies ToWorker);
      } else {
        fallback.current = buildIndex(songs);
      }
    }, 250);

    return () => clearTimeout(timer);
  }, [songs]);

  useEffect(() => {
    if (query.trim() === '') {
      setHits(null);
      return;
    }

    const id = ++nextId.current;
    const timer = setTimeout(() => {
      if (worker.current === null) {
        setHits(search(fallback.current, query));
        return;
      }

      const listener = (event: MessageEvent<FromWorker>): void => {
        const message = event.data;

        // Results from a query the user has already typed past are dropped, so a slow answer
        // can never overwrite a newer one.
        if (message.kind === 'results' && message.id === id) {
          setHits(message.hits);
          worker.current?.removeEventListener('message', listener);
        }
      };

      worker.current.addEventListener('message', listener);
      worker.current.postMessage({ kind: 'query', id, text: query } satisfies ToWorker);
    }, 120);

    return () => clearTimeout(timer);
  }, [query]);

  return hits;
}
