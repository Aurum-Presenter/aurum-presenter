<?php

declare(strict_types=1);

namespace App\Database;

use Doctrine\DBAL\Connection;

/**
 * The content tier: one SQLite file per workspace.
 *
 * Nothing here checks permissions — by the time a caller reaches this class, WorkspaceMiddleware
 * has already verified membership. That is the whole design: a handler cannot query a workspace
 * it was not granted, because it is never handed the connection.
 */
final class WorkspaceDatabase
{
    /** @var array<string, Connection> */
    private array $open = [];

    public function __construct(
        private readonly ConnectionFactory $factory,
        private readonly Migrator $migrator,
        private readonly string $directory,
        private readonly bool $autoMigrate,
    ) {
    }

    public function pathFor(string $workspaceId): string
    {
        return sprintf('%s/%s.sqlite', rtrim($this->directory, '/'), $workspaceId);
    }

    public function exists(string $workspaceId): bool
    {
        return is_file($this->pathFor($workspaceId));
    }

    /**
     * Opens the workspace file, creating and migrating it if it does not exist yet. A file that
     * is behind the current migration set is brought up to date here, so a workspace restored
     * from an old backup repairs itself rather than failing on its first query.
     */
    public function open(string $workspaceId): Connection
    {
        if (isset($this->open[$workspaceId])) {
            return $this->open[$workspaceId];
        }

        $connection = $this->factory->open($this->pathFor($workspaceId));

        if ($this->autoMigrate) {
            $this->migrator->migrate($connection);
        }

        return $this->open[$workspaceId] = $connection;
    }

    /** @return string[] every workspace id that has a database file on this host */
    public function all(): array
    {
        $files = glob(rtrim($this->directory, '/') . '/*.sqlite') ?: [];

        return array_map(static fn (string $f): string => basename($f, '.sqlite'), $files);
    }

    /** @return string[] versions applied by this call */
    public function migrate(string $workspaceId): array
    {
        return $this->migrator->migrate($this->open($workspaceId));
    }

    public function delete(string $workspaceId): void
    {
        unset($this->open[$workspaceId]);

        // WAL leaves two sidecar files; a "deleted" workspace that leaves -wal behind is a
        // workspace whose last transactions are still recoverable on disk.
        foreach (['', '-wal', '-shm'] as $suffix) {
            $file = $this->pathFor($workspaceId) . $suffix;
            if (is_file($file)) {
                unlink($file);
            }
        }
    }
}
