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

    /**
     * A band's first service should not begin with a configuration screen: every workspace file
     * is created with the theme the presenter-output spec describes — white on black, centred,
     * 8vh — and it syncs to their devices like any other record.
     */
    public function testAWorkspaceIsCreatedWithADefaultTheme(): void
    {
        $theme = $this->db->fetchAssociative('SELECT * FROM presenter_themes WHERE is_default = 1');

        self::assertIsArray($theme);
        self::assertSame('#000000', $theme['background_value']);
        self::assertSame('#ffffff', $theme['text_color']);
        self::assertSame('center', $theme['align']);
        self::assertSame(8.0, (float) $theme['font_size_vh']);
        self::assertGreaterThan(0, (int) $theme['change_seq'], 'It has to be pullable, like any other row.');
        self::assertMatchesRegularExpression('/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}Z$/', (string) $theme['updated_at']);
    }

    /** Running the migrations again must not give a workspace a second default theme. */
    public function testTheDefaultThemeIsSeededOnce(): void
    {
        (new \App\Database\Migrator(dirname(__DIR__, 2) . '/migrations/workspace'))->migrate($this->db);

        self::assertSame(1, (int) $this->db->fetchOne('SELECT COUNT(*) FROM presenter_themes'));
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
        $start = (int) $this->db->fetchOne('SELECT seq FROM sync_counter');
        $seen = [];

        for ($i = 0; $i < 25; $i++) {
            $seen[] = $transaction->run($this->db, static fn (Connection $db, int $seq): int => $seq);
        }

        self::assertSame(range($start + 1, $start + 25), $seen);
    }

    public function testRolledBackTransactionDoesNotConsumeASequenceValue(): void
    {
        $transaction = new WriteTransaction();

        $first = $transaction->run($this->db, static fn (Connection $db, int $seq): int => $seq);

        try {
            $transaction->run($this->db, static function (): never {
                throw new \RuntimeException('deliberate failure');
            });
        } catch (\RuntimeException) {
            // expected
        }

        $next = $transaction->run($this->db, static fn (Connection $db, int $seq): int => $seq);

        self::assertSame($first + 1, $next, 'A rolled-back transaction must not leave a gap.');
    }
}
