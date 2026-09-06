<?php

declare(strict_types=1);

namespace App\Database;

use Doctrine\DBAL\Connection;
use Doctrine\DBAL\DriverManager;

/**
 * Opens SQLite files with the pragmas this application depends on.
 *
 * None of the four is optional:
 *  - WAL lets readers run while the single writer holds the write lock. Without it a sync pull
 *    blocks behind a sync push.
 *  - busy_timeout is what makes PHP-FPM's multi-process model survivable: concurrent writers
 *    queue for up to five seconds instead of failing immediately with SQLITE_BUSY.
 *  - foreign_keys is OFF by default in SQLite, per connection. Every FK in the schema is
 *    decorative until this runs.
 *  - synchronous=NORMAL is the correct pairing with WAL: durable across process crashes, and
 *    only at risk in a power loss, which is the trade the sync engine already tolerates.
 */
final class ConnectionFactory
{
    public function __construct(private readonly int $busyTimeoutMs = 5000)
    {
    }

    public function open(string $path): Connection
    {
        $directory = dirname($path);
        if (! is_dir($directory) && ! @mkdir($directory, 0o775, true) && ! is_dir($directory)) {
            throw new DatabaseException(sprintf('Cannot create database directory "%s".', $directory));
        }

        $connection = DriverManager::getConnection([
            'driver' => 'pdo_sqlite',
            'path'   => $path,
        ]);

        $connection->executeQuery('PRAGMA journal_mode = WAL');
        $connection->executeStatement(sprintf('PRAGMA busy_timeout = %d', $this->busyTimeoutMs));
        $connection->executeStatement('PRAGMA foreign_keys = ON');
        $connection->executeStatement('PRAGMA synchronous = NORMAL');

        return $connection;
    }
}
