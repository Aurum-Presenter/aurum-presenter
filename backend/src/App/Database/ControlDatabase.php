<?php

declare(strict_types=1);

namespace App\Database;

use Doctrine\DBAL\Connection;

/**
 * The identity tier: accounts, credentials, sessions, workspaces, memberships, invites.
 *
 * Opened on every request, because "which workspaces may this caller open" cannot be answered
 * from a workspace file — that is the one question the per-workspace split cannot answer about
 * itself.
 */
final class ControlDatabase
{
    private ?Connection $connection = null;

    public function __construct(
        private readonly ConnectionFactory $factory,
        private readonly Migrator $migrator,
        private readonly string $path,
        private readonly bool $autoMigrate,
    ) {
    }

    public function connection(): Connection
    {
        if ($this->connection === null) {
            $this->connection = $this->factory->open($this->path);

            if ($this->autoMigrate) {
                $this->migrator->migrate($this->connection);
            }
        }

        return $this->connection;
    }

    public function path(): string
    {
        return $this->path;
    }

    /** @return string[] versions applied by this call */
    public function migrate(): array
    {
        return $this->migrator->migrate($this->factory->open($this->path));
    }
}
