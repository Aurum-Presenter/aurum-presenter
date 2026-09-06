<?php

declare(strict_types=1);

namespace App\Database;

use Doctrine\DBAL\Connection;

/**
 * Applies numbered plain-SQL migrations and records them per database file.
 *
 * There is no ORM and no doctrine-migrations here on purpose: there are two independent
 * migration sets (control and workspace) and the workspace set has to be applied to an
 * unbounded number of files. Keeping the applied set inside each file is what lets a workspace
 * restored from an old backup repair itself on open, without a central registry that could
 * disagree with the file it describes.
 */
final class Migrator
{
    public function __construct(private readonly string $migrationsPath)
    {
    }

    /**
     * @return string[] versions applied by this call, in order
     */
    public function migrate(Connection $connection): array
    {
        $this->ensureVersionTable($connection);

        $applied = $this->appliedVersions($connection);
        $pending = array_diff(array_keys($this->available()), $applied);
        sort($pending);

        $ran = [];
        foreach ($pending as $version) {
            $this->apply($connection, $version, $this->available()[$version]);
            $ran[] = $version;
        }

        return $ran;
    }

    public function pendingCount(Connection $connection): int
    {
        $this->ensureVersionTable($connection);

        return count(array_diff(array_keys($this->available()), $this->appliedVersions($connection)));
    }

    private function apply(Connection $connection, string $version, string $file): void
    {
        $sql = file_get_contents($file);
        if ($sql === false) {
            throw new DatabaseException(sprintf('Cannot read migration "%s".', $file));
        }

        // PDO::exec runs a multi-statement script; DBAL's executeStatement does not. Migrations
        // are DDL scripts, so this is the right tool even though it bypasses the DBAL layer.
        $pdo = NativePdo::of($connection);

        $connection->beginTransaction();
        try {
            $pdo->exec($sql);
            $connection->insert('schema_version', [
                'version'    => $version,
                'applied_at' => gmdate('Y-m-d\TH:i:s.v\Z'),
            ]);
            $connection->commit();
        } catch (\Throwable $e) {
            $connection->rollBack();
            throw new DatabaseException(
                sprintf('Migration "%s" failed: %s', $version, $e->getMessage()),
                previous: $e
            );
        }
    }

    private function ensureVersionTable(Connection $connection): void
    {
        $connection->executeStatement(
            'CREATE TABLE IF NOT EXISTS schema_version (
                version    TEXT PRIMARY KEY,
                applied_at TEXT NOT NULL
            )'
        );
    }

    /** @return string[] */
    private function appliedVersions(Connection $connection): array
    {
        return $connection->fetchFirstColumn('SELECT version FROM schema_version ORDER BY version');
    }

    /** @return array<string, string> version => absolute path */
    private function available(): array
    {
        $files = glob(rtrim($this->migrationsPath, '/') . '/*.sql') ?: [];
        $map = [];

        foreach ($files as $file) {
            $map[basename($file, '.sql')] = $file;
        }

        ksort($map);

        return $map;
    }
}
