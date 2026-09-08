/**
 * Reading what the app left on the device.
 *
 * A handful of specs assert on the device's own state rather than on the screen — that a mark
 * survived a file being replaced, that a released pin really stopped being wanted, that a
 * workspace came off the device entirely. Those assertions have to reach into IndexedDB and the
 * origin private file system, so the reaching is done here, in one place, and named.
 *
 * These names are part of the contract a rewrite has to keep: the database is `aurum-<workspace>`
 * with one store per synced table, and the same keys live in localStorage.
 */

export function databaseFor(workspaceId) {
  return `aurum-${workspaceId}`;
}

/** Every `aurum-*` database this origin holds. */
export async function databases(page) {
  return page.evaluate(async () => {
    const found = indexedDB.databases ? await indexedDB.databases() : [];

    return found.map((entry) => entry.name ?? '').filter((name) => name.startsWith('aurum-'));
  });
}

/** Every row of one store, as plain objects. */
export async function rows(page, workspaceId, store) {
  return page.evaluate(async ([name, table]) => new Promise((resolve, reject) => {
    const open = indexedDB.open(name);

    open.onerror = () => reject(new Error(`cannot open ${name}`));
    open.onsuccess = () => {
      const database = open.result;

      if (! database.objectStoreNames.contains(table)) {
        database.close();
        resolve([]);
        return;
      }

      const request = database.transaction(table, 'readonly').objectStore(table).getAll();
      request.onsuccess = () => { database.close(); resolve(request.result); };
      request.onerror = () => { database.close(); reject(new Error(`cannot read ${table}`)); };
    };
  }), [databaseFor(workspaceId), store]);
}

export async function row(page, workspaceId, store, key) {
  return page.evaluate(async ([name, table, id]) => new Promise((resolve, reject) => {
    const open = indexedDB.open(name);

    open.onerror = () => reject(new Error(`cannot open ${name}`));
    open.onsuccess = () => {
      const database = open.result;
      const request = database.transaction(table, 'readonly').objectStore(table).get(id);
      request.onsuccess = () => { database.close(); resolve(request.result ?? null); };
      request.onerror = () => { database.close(); reject(new Error(`cannot read ${table}`)); };
    };
  }), [databaseFor(workspaceId), store, key]);
}

/** Drops every row whose key starts with a prefix — used to simulate a device losing its files. */
export async function forgetFiles(page, workspaceId, prefix) {
  await page.evaluate(async ([name, start]) => new Promise((resolve) => {
    const open = indexedDB.open(name);

    open.onsuccess = () => {
      const database = open.result;
      const request = database.transaction('files', 'readwrite').objectStore('files').openCursor();

      request.onsuccess = () => {
        const cursor = request.result;

        if (cursor === null) {
          database.close();
          resolve();
          return;
        }

        if (String(cursor.key).startsWith(start)) {
          cursor.delete();
        }

        cursor.continue();
      };
    };
  }), [databaseFor(workspaceId), prefix]);
}

/** The origin private file system entries for a workspace, or null when the directory is gone. */
export async function storedFiles(page, workspaceId) {
  return page.evaluate(async (name) => {
    try {
      const root = await navigator.storage.getDirectory();
      const directory = await root.getDirectoryHandle(name);
      const names = [];

      for await (const key of directory.keys()) {
        names.push(key);
      }

      return names;
    } catch {
      return null;
    }
  }, databaseFor(workspaceId));
}

export async function localStorageValue(page, key) {
  return page.evaluate((name) => localStorage.getItem(name), key);
}
