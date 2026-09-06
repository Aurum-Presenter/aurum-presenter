<?php

declare(strict_types=1);

namespace App\Database;

use Doctrine\DBAL\Connection;
use Throwable;

/**
 * Runs a closure inside `BEGIN IMMEDIATE`, and hands it the workspace's next change sequence.
 *
 * Why not DBAL's beginTransaction(): it issues a plain `BEGIN`, which in SQLite is *deferred* —
 * the write lock is taken lazily at the first write. Two deferred transactions that both read
 * the counter before either writes will produce the same sequence value, and one of them will
 * then fail with SQLITE_BUSY on upgrade rather than waiting. `BEGIN IMMEDIATE` takes the write
 * lock up front, so writers queue on busy_timeout and the counter is strictly serialised.
 *
 * This is the SQLite replacement for the per-workspace Postgres sequence, and it is stronger in
 * one respect: a sequence burns a value on rollback, whereas this counter is rolled back with
 * the rest of the transaction, so `change_seq` has no gaps.
 *
 * Because it drives the transaction through the native PDO handle, callers must not mix it with
 * DBAL's own transaction methods on the same connection.
 */
final class WriteTransaction
{
    /**
     * @template T
     * @param callable(Connection, int): T $work receives the connection and the next change_seq
     * @return T
     */
    public function run(Connection $connection, callable $work): mixed
    {
        $pdo = NativePdo::of($connection);
        $pdo->exec('BEGIN IMMEDIATE');

        try {
            $seq = $this->nextSequence($connection);
            $result = $work($connection, $seq);
            $pdo->exec('COMMIT');

            return $result;
        } catch (Throwable $e) {
            $pdo->exec('ROLLBACK');

            throw $e;
        }
    }

    /**
     * Reserves a block of sequence values in one bump, for a batch push that writes many rows.
     * Returns the first value; the caller may use $count consecutive values from it.
     *
     * @template T
     * @param callable(Connection, int): T $work receives the connection and the first change_seq
     * @return T
     */
    public function runBatch(Connection $connection, int $count, callable $work): mixed
    {
        $pdo = NativePdo::of($connection);
        $pdo->exec('BEGIN IMMEDIATE');

        try {
            $last = (int) $connection->fetchOne('SELECT seq FROM sync_counter WHERE id = 1');
            $first = $last + 1;
            $connection->executeStatement('UPDATE sync_counter SET seq = ? WHERE id = 1', [$last + max($count, 0)]);

            $result = $work($connection, $first);
            $pdo->exec('COMMIT');

            return $result;
        } catch (Throwable $e) {
            $pdo->exec('ROLLBACK');

            throw $e;
        }
    }

    private function nextSequence(Connection $connection): int
    {
        $connection->executeStatement('UPDATE sync_counter SET seq = seq + 1 WHERE id = 1');

        return (int) $connection->fetchOne('SELECT seq FROM sync_counter WHERE id = 1');
    }
}
