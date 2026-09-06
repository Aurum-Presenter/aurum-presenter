<?php

declare(strict_types=1);

namespace App\Support;

/**
 * Thin getenv() wrapper. Config files are plain PHP, so this exists only to give them a
 * single place that understands "unset" versus "set to an empty string" and coerces the
 * handful of scalar shapes we actually use.
 */
final class Env
{
    public static function string(string $key, ?string $default = null): ?string
    {
        $value = getenv($key);

        return $value === false || $value === '' ? $default : $value;
    }

    public static function int(string $key, int $default): int
    {
        $value = self::string($key);

        return $value === null ? $default : (int) $value;
    }

    public static function bool(string $key, bool $default): bool
    {
        $value = self::string($key);

        return $value === null ? $default : in_array(strtolower($value), ['1', 'true', 'yes', 'on'], true);
    }
}
