/**
 * One syncing window per device.
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
