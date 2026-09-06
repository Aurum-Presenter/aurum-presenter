<?php

declare(strict_types=1);

namespace App\Support;

use DateTimeImmutable;
use DateTimeZone;

/**
 * All timestamps are ISO-8601 UTC with milliseconds, stored as TEXT. The format is fixed
 * width, so SQLite's lexicographic string comparison is chronological comparison — which is
 * what lets `updated_at > ?` work without a date type.
 */
final class Clock
{
    public const string FORMAT = 'Y-m-d\TH:i:s.v\Z';

    public function now(): string
    {
        return (new DateTimeImmutable('now', new DateTimeZone('UTC')))->format(self::FORMAT);
    }

    public function plusSeconds(int $seconds): string
    {
        return (new DateTimeImmutable('now', new DateTimeZone('UTC')))
            ->modify(sprintf('+%d seconds', $seconds))
            ->format(self::FORMAT);
    }

    public function minusSeconds(int $seconds): string
    {
        return (new DateTimeImmutable('now', new DateTimeZone('UTC')))
            ->modify(sprintf('-%d seconds', $seconds))
            ->format(self::FORMAT);
    }

    public function isPast(?string $timestamp): bool
    {
        return $timestamp !== null && $timestamp < $this->now();
    }
}
