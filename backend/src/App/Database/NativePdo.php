<?php

declare(strict_types=1);

namespace App\Database;

use Doctrine\DBAL\Connection;
use PDO;

/**
 * DBAL declares getNativeConnection() as `object|resource`, because a driver may wrap anything.
 * Two places here legitimately need the real PDO handle — multi-statement migration scripts,
 * and `BEGIN IMMEDIATE`, which DBAL's transaction API cannot express — so the narrowing happens
 * once, with a real check rather than a silencing cast.
 */
final class NativePdo
{
    public static function of(Connection $connection): PDO
    {
        $native = $connection->getNativeConnection();

        if (! $native instanceof PDO) {
            throw new DatabaseException(sprintf(
                'Expected a PDO connection, got %s. This application requires the pdo_sqlite driver.',
                get_debug_type($native),
            ));
        }

        return $native;
    }
}
