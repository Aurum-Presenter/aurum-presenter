<?php

declare(strict_types=1);

namespace App\Support;

use Ramsey\Uuid\Uuid as RamseyUuid;

/**
 * UUIDv7 everywhere. Two properties the system depends on: a client can mint an id while
 * offline without ever needing it remapped on the server, and the id sorts chronologically,
 * so creation-ordered listings need no separate timestamp index.
 */
final class Uuid
{
    public static function generate(): string
    {
        return RamseyUuid::uuid7()->toString();
    }

    public static function isValid(string $value): bool
    {
        return RamseyUuid::isValid($value);
    }
}
