import type { Table } from 'dexie';
import { api, ApiError } from '../api/client';
import { SYNCED_TABLES, type OutboxOp, type SyncedTable, type WorkspaceDb } from '../db/schema';
import { uuidv7 } from '../db/uuid';
import { asSoleWorker } from './lock';

export interface PushResult {
  op_id: string;
  status: 'applied' | 'duplicate' | 'rejected';
  change_seq?: number;
  conflicts?: string[];
  code?: string;
  error?: string;
}

/**
 * The sync engine: every write lands locally first, and the UI never waits for the network.
 *
 * The order below is deliberate — drain, then pull. Pulling first would hand back the server's
 * older version of a record this device has already changed locally but not yet pushed, and the
 * apply step would overwrite the newer local edit.
 */
export class SyncEngine {
  private running = false;

  constructor(
    private readonly db: WorkspaceDb,
    private readonly workspaceId: string,
  ) {}

  /**
   * Records a local change: writes it to the local table and appends an operation to the
   * outbox, in one transaction. If the two could diverge, a device could show a change it will
   * never send, or send one it does not show.
   */
  async record(
    table: SyncedTable,
    recordId: string,
    op: 'upsert' | 'delete',
    payload: Record<string, unknown>,
  ): Promise<void> {
    await this.db.transaction('rw', this.store(table), this.db.outbox, async () => {
      const existing = await this.store(table).get(recordId);
      const now = new Date().toISOString();

      if (op === 'delete') {
        await this.store(table).update(recordId, { deleted_at: now });
      } else {
        await this.store(table).put({
          ...(existing === undefined ? blankColumns(table) : existing),
          ...payload,
          id: recordId,
          updated_at: now,
          deleted_at: null,
        });
      }

      const entry: OutboxOp = {
        op_id: uuidv7(),
        table,
        record_id: recordId,
        op,
        payload,
        base_updated_at: (existing as { updated_at?: string } | undefined)?.updated_at ?? null,
        attempts: 0,
        last_error: null,
        status: 'pending',
        created_at: now,
      };

      await this.db.outbox.add(entry);
    });
  }

  /**
   * Indexing the database by a union of table names produces a union of EntityTable types
   * whose overloads are not mutually compatible, so TypeScript refuses to call bulkPut on it.
   * The sync engine is deliberately generic over tables — it moves opaque rows — so the row
   * type is erased once here rather than cast at every call site.
   */
  private store(table: SyncedTable): Table<Record<string, unknown>, string> {
    return this.db[table] as unknown as Table<Record<string, unknown>, string>;
  }

  async pendingCount(): Promise<number> {
    return this.db.outbox.where('status').equals('pending').count();
  }

  /** Operations the server refused. They wait for a person, not for a timer. */
  async parked(): Promise<OutboxOp[]> {
    return this.db.outbox.where('status').equals('parked').sortBy('seq');
  }

  /** Puts a parked operation back in the queue, after whatever blocked it has been fixed. */
  async retry(seq: number): Promise<void> {
    await this.db.outbox.update(seq, { status: 'pending', last_error: null });
  }

  /** Throws an operation away. The local record keeps whatever it has; only the push is lost. */
  async discard(seq: number): Promise<void> {
    await this.db.outbox.delete(seq);
  }

  async status(): Promise<{ pending: number; parked: number; lastPull: string | null; lastPush: string | null }> {
    const state = await this.db.sync_state.get('watermark');

    return {
      pending: await this.db.outbox.where('status').equals('pending').count(),
      parked: await this.db.outbox.where('status').equals('parked').count(),
      lastPull: state?.last_pull_at ?? null,
      lastPush: state?.last_push_at ?? null,
    };
  }

  async sync(): Promise<{ pushed: number; pulled: number }> {
    if (this.running || !navigator.onLine) {
      return { pushed: 0, pulled: 0 };
    }

    // One window at a time across the whole device, not just one pass at a time in this one.
    return asSoleWorker(`aurum-sync-${this.workspaceId}`, async () => {
      this.running = true;

      try {
        const pushed = await this.drain();
        const pulled = await this.pull();
        return { pushed, pulled };
      } finally {
        this.running = false;
      }
    }, { pushed: 0, pulled: 0 });
  }

  private async drain(): Promise<number> {
    const pending = await this.db.outbox
      .where('status').equals('pending')
      .sortBy('seq');

    if (pending.length === 0) {
      return 0;
    }

    // Batched, but still in strict local order, so a create always reaches the server before
    // the update that depends on it.
    const batch = pending.slice(0, 200);

    let response: { results: PushResult[]; change_seq: number };
    try {
      response = await api(`/workspaces/${this.workspaceId}/sync/push`, {
        method: 'POST',
        body: JSON.stringify({
          ops: batch.map((op) => ({
            op_id: op.op_id,
            table: op.table,
            record_id: op.record_id,
            op: op.op,
            payload: op.payload,
            base_updated_at: op.base_updated_at,
          })),
        }),
      });
    } catch (error) {
      // Business rule 12: an expired session pauses the queue, it does not park it. Parking
      // asks a person to decide something, and "sign in again" is not a decision about their
      // work — the same batch goes out on the first pass after they do.
      if (error instanceof ApiError && !error.isRetryable && error.status !== 401) {
        // A permanent error parks the batch rather than retrying it forever. Nothing is
        // discarded: a parked op waits for an explicit user decision.
        await this.db.outbox.bulkUpdate(
          batch.map((op) => ({
            key: op.seq!,
            changes: { status: 'parked' as const, last_error: error.message },
          })),
        );
      }
      throw error;
    }

    const byId = new Map(response.results.map((r) => [r.op_id, r]));
    let applied = 0;

    for (const op of batch) {
      const result = byId.get(op.op_id);

      if (result === undefined) {
        continue;
      }

      if (result.status === 'rejected') {
        await this.db.outbox.update(op.seq!, {
          status: 'parked',
          last_error: result.error ?? result.code ?? 'rejected',
          attempts: op.attempts + 1,
        });
        continue;
      }

      if (result.conflicts?.length) {
        await this.db.conflicts.bulkPut(
          result.conflicts.map((field) => ({
            id: uuidv7(),
            table: op.table,
            record_id: op.record_id,
            field,
            losing_value: null,
            at: new Date().toISOString(),
            reviewed_at: null,
          })),
        );
      }

      await this.db.outbox.delete(op.seq!);
      applied++;
    }

    if (applied > 0) {
      const state = await this.db.sync_state.get('watermark');

      await this.db.sync_state.put({
        key: 'watermark',
        change_seq: state?.change_seq ?? 0,
        last_pull_at: state?.last_pull_at ?? null,
        last_push_at: new Date().toISOString(),
      });
    }

    return applied;
  }

  private async pull(): Promise<number> {
    const state = await this.db.sync_state.get('watermark');
    const since = state?.change_seq ?? 0;

    const response = await api<{
      change_seq: number;
      tables: Record<SyncedTable, Record<string, unknown>[]>;
      has_more: boolean;
    }>(`/workspaces/${this.workspaceId}/sync/pull?since=${since}`);

    let applied = 0;

    await this.db.transaction(
      'rw',
      [...SYNCED_TABLES.map((t) => this.store(t)), this.db.sync_state],
      async () => {
        for (const table of SYNCED_TABLES) {
          const rows = response.tables[table] ?? [];

          if (rows.length > 0) {
            await this.store(table).bulkPut(rows);
            applied += rows.length;
          }
        }

        await this.db.sync_state.put({
          key: 'watermark',
          change_seq: response.change_seq,
          last_pull_at: new Date().toISOString(),
          last_push_at: state?.last_push_at ?? null,
        });
      },
    );

    // A capped page means the server has more waiting; keep going rather than leaving the
    // device a page behind until the next tick.
    if (response.has_more) {
      applied += await this.pull();
    }

    return applied;
  }
}

/**
 * Server-owned columns, present as null from the moment a row is created on a device.
 *
 * They are filled in by the server and arrive on a later pull, so on the device that made the
 * row they would otherwise be *undefined* rather than null — and `sheet.sha256 !== null` is
 * true for undefined. That is how the device holding the only copy of a file came to be told
 * the file had not been downloaded.
 */
const SERVER_OWNED: Partial<Record<SyncedTable, string[]>> = {
  sheets: ['sha256', 'size', 'uploaded_at', 'page_count', 'pages_changed_at'],
};

function blankColumns(table: SyncedTable): Record<string, unknown> {
  const blank: Record<string, unknown> = { change_seq: 0, updated_by: null };

  for (const column of SERVER_OWNED[table] ?? []) {
    blank[column] = null;
  }

  return blank;
}
