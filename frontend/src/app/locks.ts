/**
 * Locks that hold across every window of the app on one device.
 *
 * The app is routinely open three times at once — library, control surface, stage — and each
 * one runs the same thirty-second tick. Without this they would all drain the same outbox and
 * pull the same deltas: harmless, because pushes are idempotent and the watermark is
 * monotonic, but three times the requests for one device's work (offline-sync acceptance
 * criterion 8).
 *
 * The lock is held only while the pass runs, so a window that is closed mid-sync hands it
 * straight to the next one rather than leaving the device stuck.
 */
export async function asSoleWorker<T>(name: string, pass: () => Promise<T>, skipped: T): Promise<T> {
  const locks = navigator.locks as LockManager | undefined;

  if (locks === undefined) {
    // Safari before 15.4, and any context without the API. Falling back to running the pass is
    // the right way round: syncing twice is a waste, not syncing at all is data loss.
    return pass();
  }

  let result = skipped;

  await locks.request(name, { ifAvailable: true }, async (lock) => {
    if (lock === null) {
      return;
    }

    result = await pass();
  });

  return result;
}

/**
 * Waits its turn rather than skipping: for work every caller must do, but only one at a time.
 *
 * The refresh cookie is the case that matters. Three windows opening at once each ask for a new
 * access token, and rotation means the second use of the same refresh token is theft as far as
 * the server is concerned — it revokes the whole family and signs the musician out everywhere,
 * mid-service, for the crime of having the control surface and the stage view open. Taking
 * turns means each window presents the cookie the last one left behind.
 */
export async function inTurn<T>(name: string, work: () => Promise<T>): Promise<T> {
  const locks = navigator.locks as LockManager | undefined;

  if (locks === undefined) {
    return work();
  }

  return await locks.request(name, work);
}
