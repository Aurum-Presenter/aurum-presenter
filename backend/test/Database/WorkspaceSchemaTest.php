<?php

declare(strict_types=1);

namespace AppTest\Database;

use App\Database\WriteTransaction;
use AppTest\WorkspaceTestCase;
use Doctrine\DBAL\Connection;

final class WorkspaceSchemaTest extends WorkspaceTestCase
{
    /**
     * The structural guarantee that replaced row-level security. If a workspace_id column ever
     * reappears, a query can start filtering on it — and a query that filters can forget to.
     */
    public function testNoContentTableCarriesAWorkspaceId(): void
    {
        $tables = $this->db->fetchFirstColumn(
            "SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%'"
        );

        self::assertNotEmpty($tables);

        foreach ($tables as $table) {
            $columns = $this->db->fetchFirstColumn(sprintf('SELECT name FROM pragma_table_info(%s)', $this->db->quote($table)));

            self::assertNotContains(
                'workspace_id',
                $columns,
                sprintf('Table "%s" reintroduced a workspace_id column.', $table)
            );
        }
    }

    public function testPragmasAreApplied(): void
    {
        self::assertSame('wal', strtolower((string) $this->db->fetchOne('PRAGMA journal_mode')));
        self::assertSame(1, (int) $this->db->fetchOne('PRAGMA foreign_keys'));
    }

    public function testMigrationIsIdempotent(): void
    {
        $migrator = new \App\Database\Migrator(dirname(__DIR__, 2) . '/migrations/workspace');

        self::assertSame(0, $migrator->pendingCount($this->db));
        self::assertSame([], $migrator->migrate($this->db));
    }

    /**
     * The counter is the SQLite replacement for the per-workspace Postgres sequence: strictly
     * increasing, and — unlike a sequence — gap-free, because a rollback takes the counter with
     * it rather than burning a value.
     */
    public function testChangeSequenceIsMonotonicAndGapFree(): void
    {
        $transaction = new WriteTransaction();
        $seen = [];

        for ($i = 0; $i < 25; $i++) {
            $seen[] = $transaction->run($this->db, static fn (Connection $db, int $seq): int => $seq);
        }

        self::assertSame(range(1, 25), $seen);
    }

    public function testRolledBackTransactionDoesNotConsumeASequenceValue(): void
    {
        $transaction = new WriteTransaction();

        $transaction->run($this->db, static fn (Connection $db, int $seq): int => $seq);

        try {
            $transaction->run($this->db, static function (): never {
                throw new \RuntimeException('deliberate failure');
            });
        } catch (\RuntimeException) {
            // expected
        }

        $next = $transaction->run($this->db, static fn (Connection $db, int $seq): int => $seq);

        self::assertSame(2, $next, 'A rolled-back transaction must not leave a gap.');
    }
}
