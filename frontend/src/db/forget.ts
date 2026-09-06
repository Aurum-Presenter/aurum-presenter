import Dexie from 'dexie';

/**
 * Removing a workspace from the device once the account no longer has it.
 *
 * Losing access to a band's workspace has to mean losing the copy too: the songs, the charts
 * and the pinned PDFs are the band's, and they should not sit on a phone that has been removed
 * from it (workspaces business rule 7). Everything is local here — there is nothing to sync,
 * because the server has already stopped answering for this workspace.
 */
export async function forgetWorkspacesExcept(keep: string[]): Promise<string[]> {
  const kept = new Set(keep);
  const forgotten: string[] = [];

  for (const id of await localWorkspaceIds()) {
    if (kept.has(id)) {
      continue;
    }

    await Dexie.delete(`aurum-${id}`).catch(() => undefined);
    await removeFiles(id);

    try {
      localStorage.removeItem(`aurum.released.${id}`);
    } catch {
      // No storage, nothing to clean up.
    }

    forgotten.push(id);
  }

  return forgotten;
}

async function localWorkspaceIds(): Promise<string[]> {
  const databases = (indexedDB as IDBFactory & { databases?: () => Promise<{ name?: string }[]> }).databases;

  if (databases === undefined) {
    // Firefox: there is no way to enumerate, so nothing can be tidied up. The workspace is gone
    // from the switcher either way, and the server will not answer for it.
    return [];
  }

  try {
    return (await databases.call(indexedDB))
      .map((entry) => entry.name ?? '')
      .filter((name) => name.startsWith('aurum-'))
      .map((name) => name.slice('aurum-'.length))
      .filter((id) => id !== '' && id !== 'null' && id !== 'undefined');
  } catch {
    return [];
  }
}

async function removeFiles(workspaceId: string): Promise<void> {
  try {
    const storage = navigator.storage as unknown as {
      getDirectory?: () => Promise<{ removeEntry: (name: string, options?: { recursive?: boolean }) => Promise<void> }>;
    };

    const root = await storage.getDirectory?.();
    await root?.removeEntry(`aurum-${workspaceId}`, { recursive: true });
  } catch {
    // No origin private file system, or nothing stored there for this workspace.
  }
}
