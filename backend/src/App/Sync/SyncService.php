<?php

declare(strict_types=1);

namespace App\Sync;

use App\Database\WriteTransaction;
use App\Enum\WorkspaceRole;
use App\Http\ApiException;
use App\Support\Clock;
use App\Support\Uuid;
use Doctrine\DBAL\Connection;
use Doctrine\DBAL\Exception as DbalException;

/**
 * The whole data API: a batch push and a delta pull.
 *
 * Per-table REST endpoints were folded into these two, because every table's "endpoint" in the
 * feature documents was already described as either the delta pull or the sync-queue upsert.
 */
final class SyncService
{
    public function __construct(
        private readonly WriteTransaction $transaction,
        private readonly Clock $clock,
    ) {
    }

    /**
     * @param list<array<string, mixed>> $ops
     * @return array{results: list<array<string, mixed>>, change_seq: int}
     */
    public function push(Connection $db, array $ops, string $userId, WorkspaceRole $role): array
    {
        if ($ops === []) {
            return ['results' => [], 'change_seq' => $this->currentSeq($db)];
        }

        if (count($ops) > 500) {
            throw ApiException::unprocessable('A push may carry at most 500 operations.', [], 'batch_too_large');
        }

        // One BEGIN IMMEDIATE for the whole batch: the sequence values are contiguous, and a
        // batch that fails part-way leaves the workspace exactly as it was.
        return $this->transaction->runBatch($db, count($ops), function (Connection $db, int $firstSeq) use ($ops, $userId, $role): array {
            $results = [];
            $seq = $firstSeq;
            $now = $this->clock->now();

            foreach ($ops as $index => $op) {
                try {
                    $results[] = $this->applyOne($db, $op, $userId, $role, $seq, $now);
                    $seq++;
                } catch (DbalException $e) {
                    // A row the schema refuses — a set item that is both a song and a text item,
                    // a capo outside 0-11. That is one bad operation, not a broken batch, so it
                    // parks like any other permanent failure rather than failing the push.
                    $results[] = [
                        'op_id'  => is_string($op['op_id'] ?? null) ? $op['op_id'] : sprintf('index-%d', $index),
                        'status' => 'rejected',
                        'code'   => 'constraint_violation',
                        'error'  => 'The server refused this row: ' . $e->getMessage(),
                    ];
                    $seq++;
                } catch (ApiException $e) {
                    // A permanent failure parks that one operation; the rest of the batch still
                    // applies. The client's outbox needs the op id back to know what to park.
                    $results[] = [
                        'op_id'  => is_string($op['op_id'] ?? null) ? $op['op_id'] : sprintf('index-%d', $index),
                        'status' => 'rejected',
                        'code'   => $e->errorCode(),
                        'error'  => $e->getMessage(),
                    ];
                    $seq++;
                }
            }

            return ['results' => $results, 'change_seq' => $this->currentSeq($db)];
        });
    }

    /**
     * @param array<string, mixed> $op
     * @return array<string, mixed>
     */
    private function applyOne(Connection $db, array $op, string $userId, WorkspaceRole $role, int $seq, string $now): array
    {
        $opId = $this->requireUuid($op, 'op_id');
        $table = is_string($op['table'] ?? null) ? $op['table'] : '';
        $recordId = $this->requireUuid($op, 'record_id');
        $kind = is_string($op['op'] ?? null) ? $op['op'] : 'upsert';

        SyncSchema::assertKnownTable($table);
        $this->assertMayWrite($table, $role, $op, $userId);

        // Idempotency: a retried batch after a lost response must not apply twice.
        $seen = $db->fetchOne('SELECT 1 FROM applied_ops WHERE op_id = ?', [$opId]);

        if ($seen !== false) {
            return ['op_id' => $opId, 'status' => 'duplicate'];
        }

        $existing = $db->fetchAssociative(
            sprintf('SELECT * FROM %s WHERE id = ?', $table),
            [$recordId]
        ) ?: null;

        $conflicts = [];

        if ($kind === 'delete') {
            $this->applyDelete($db, $table, $recordId, $existing, $seq, $now, $userId);
        } else {
            $conflicts = $this->applyUpsert($db, $table, $recordId, $existing, $op, $seq, $now, $userId);
        }

        $db->insert('applied_ops', ['op_id' => $opId, 'user_id' => $userId, 'applied_at' => $now]);

        return array_filter([
            'op_id'      => $opId,
            'status'     => 'applied',
            'change_seq' => $seq,
            'conflicts'  => $conflicts ?: null,
        ], static fn ($v) => $v !== null);
    }

    /**
     * @param array<string, mixed>|null $existing
     * @param array<string, mixed>      $op
     * @return list<string> fields whose previous value was preserved in sync_conflicts
     */
    private function applyUpsert(
        Connection $db,
        string $table,
        string $recordId,
        ?array $existing,
        array $op,
        int $seq,
        string $now,
        string $userId,
    ): array {
        $payload = SyncSchema::filter($table, is_array($op['payload'] ?? null) ? $op['payload'] : []);
        $payload = array_map(
            static fn (mixed $v): mixed => is_array($v) ? json_encode($v, JSON_THROW_ON_ERROR) : $v,
            $payload
        );

        $sync = [
            'updated_at' => $now,
            'change_seq' => $seq,
            'updated_by' => $userId,
            'deleted_at' => null,
        ];

        if ($existing === null) {
            $this->keepOneDefaultArrangement($db, $table, $recordId, $payload, $seq, $now, $userId);
            $db->insert($table, ['id' => $recordId] + $payload + $sync);

            return [];
        }

        // A tombstone always wins over a concurrent edit (sync business rule 6). Resurrecting a
        // deleted record is not something a stale offline edit gets to do by accident.
        if ($existing['deleted_at'] !== null) {
            return [];
        }

        $conflicts = $this->recordConflicts($db, $table, $recordId, $existing, $payload, $op, $now, $userId);

        $this->keepOneDefaultArrangement($db, $table, $recordId, $payload, $seq, $now, $userId);
        $db->update($table, $payload + $sync, ['id' => $recordId]);

        return $conflicts;
    }

    /**
     * Exactly one default arrangement per song.
     *
     * Two people offline can each make a different arrangement the default, and both are
     * legitimate edits — so the later one demotes the others rather than being refused. A
     * rejected push would be a change a musician made and lost, which is the one thing sync is
     * not allowed to do. The demotion carries this operation's sequence number, so every device
     * pulls it as part of the same change.
     *
     * @param array<string, mixed> $payload
     */
    private function keepOneDefaultArrangement(
        Connection $db,
        string $table,
        string $recordId,
        array $payload,
        int $seq,
        string $now,
        string $userId,
    ): void {
        if ($table !== 'arrangements' || (int) ($payload['is_default'] ?? 0) !== 1) {
            return;
        }

        // From the payload when the row is being created, from the stored row when it is being
        // updated: a push that only flips the flag does not carry the song.
        $songId = is_string($payload['song_id'] ?? null)
            ? $payload['song_id']
            : $db->fetchOne('SELECT song_id FROM arrangements WHERE id = ?', [$recordId]);

        if (! is_string($songId) || $songId === '') {
            return;
        }

        $db->executeStatement(
            'UPDATE arrangements
                SET is_default = 0, updated_at = ?, change_seq = ?, updated_by = ?
              WHERE song_id = ?
                AND id <> ?
                AND is_default = 1',
            [$now, $seq, $userId, $songId, $recordId],
        );
    }

    /**
     * A field is in conflict when the stored row moved on after the base the client edited from,
     * and the stored value differs from what is arriving. The incoming write still wins — it is
     * the later one by the server's clock — but the value it displaces is kept verbatim so the
     * conflict review panel can offer it back. Nothing is silently destroyed.
     *
     * @param array<string, mixed> $existing
     * @param array<string, mixed> $payload
     * @param array<string, mixed> $op
     * @return list<string>
     */
    private function recordConflicts(
        Connection $db,
        string $table,
        string $recordId,
        array $existing,
        array $payload,
        array $op,
        string $now,
        string $userId,
    ): array {
        $base = is_string($op['base_updated_at'] ?? null) ? $op['base_updated_at'] : null;

        if ($base === null || (string) $existing['updated_at'] <= $base) {
            return [];
        }

        $conflicted = [];

        foreach ($payload as $field => $incoming) {
            $current = $existing[$field] ?? null;

            if ((string) $current === (string) $incoming) {
                continue;
            }

            $db->insert('sync_conflicts', [
                'id'           => Uuid::generate(),
                'table_name'   => $table,
                'record_id'    => $recordId,
                'field'        => $field,
                'losing_value' => is_scalar($current) || $current === null ? $current : (string) $current,
                'losing_user'  => $existing['updated_by'] ?? null,
                'at'           => $now,
            ]);

            $conflicted[] = $field;
        }

        return $conflicted;
    }

    /** @param array<string, mixed>|null $existing */
    private function applyDelete(
        Connection $db,
        string $table,
        string $recordId,
        ?array $existing,
        int $seq,
        string $now,
        string $userId,
    ): void {
        if ($existing === null) {
            // Deleting something this device never synced is not an error: the outcome the
            // client wants — the record does not exist — already holds.
            return;
        }

        $db->update($table, [
            'deleted_at' => $now,
            'updated_at' => $now,
            'change_seq' => $seq,
            'updated_by' => $userId,
        ], ['id' => $recordId]);
    }

    /**
     * @param string[]|null $tables
     * @return array{change_seq: int, tables: array<string, list<array<string, mixed>>>, has_more: bool}
     */
    public function pull(Connection $db, int $since, string $userId, ?array $tables = null, int $limit = 1000): array
    {
        $requested = $tables === null || $tables === []
            ? SyncSchema::tables()
            : array_values(array_intersect($tables, SyncSchema::tables()));

        $result = [];
        $hasMore = false;

        foreach ($requested as $table) {
            // Preferences are personal — a preferred key, a capo, a font size. Every member has
            // their own row for the same song, and no member is ever handed anyone else's.
            $mine = $table === 'preferences' ? ' AND user_id = ?' : '';

            $rows = $db->fetchAllAssociative(
                sprintf(
                    'SELECT * FROM %s WHERE change_seq > ?%s ORDER BY change_seq LIMIT ?',
                    $table,
                    $mine
                ),
                $mine === '' ? [$since, $limit + 1] : [$since, $userId, $limit + 1]
            );

            if (count($rows) > $limit) {
                $hasMore = true;
                array_pop($rows);
            }

            $result[$table] = $rows;
        }

        return [
            'change_seq' => $this->currentSeq($db),
            'tables'     => $result,
            'has_more'   => $hasMore,
        ];
    }

    /**
     * Viewers may write only their own per-user rows. Everything a band member would see must
     * pass the workspace.write check the route already applied for editors and owners.
     *
     * @param array<string, mixed> $op
     */
    private function assertMayWrite(string $table, WorkspaceRole $role, array $op, string $userId): void
    {
        $payload = is_array($op['payload'] ?? null) ? $op['payload'] : [];

        // Not a role question: an owner has no more business writing another member's preferred
        // key than a viewer does.
        if ($table === 'preferences' && ($payload['user_id'] ?? $userId) !== $userId) {
            throw ApiException::forbidden('Preferences can only be written for yourself.', 'not_your_row');
        }

        if ($role !== WorkspaceRole::Viewer) {
            return;
        }

        if (! SyncSchema::isViewerWritable($table)) {
            throw ApiException::forbidden(
                sprintf('Your role (viewer) cannot change %s.', $table),
                'insufficient_role'
            );
        }

        if ($table === 'annotations') {
            if (($payload['scope'] ?? 'personal') !== 'personal') {
                throw ApiException::forbidden('Shared annotations need editor access.', 'insufficient_role');
            }

            if (($payload['author_id'] ?? $userId) !== $userId) {
                throw ApiException::forbidden('Annotations can only be written for yourself.', 'not_your_row');
            }
        }
    }

    /** @param array<string, mixed> $op */
    private function requireUuid(array $op, string $key): string
    {
        $value = $op[$key] ?? null;

        if (! is_string($value) || ! Uuid::isValid($value)) {
            throw ApiException::unprocessable(
                sprintf('"%s" must be a UUID.', $key),
                ['field' => $key],
                'validation_failed'
            );
        }

        return $value;
    }

    private function currentSeq(Connection $db): int
    {
        return (int) $db->fetchOne('SELECT seq FROM sync_counter WHERE id = 1');
    }
}
