<?php

declare(strict_types=1);

namespace App\Auth;

use SensitiveParameter;

/**
 * Ten single-use codes, shown exactly once at enrolment and stored only as hashes.
 * Codes are formatted in two groups so they are transcribable from a printout.
 */
final class RecoveryCodeService
{
    public const int COUNT = 10;

    public function __construct(private readonly string $signingKey)
    {
    }

    /** @return string[] plaintext codes — the only moment they exist */
    public function generate(int $count = self::COUNT): array
    {
        $codes = [];

        for ($i = 0; $i < $count; $i++) {
            $raw = strtoupper(bin2hex(random_bytes(5)));
            $codes[] = substr($raw, 0, 5) . '-' . substr($raw, 5, 5);
        }

        return $codes;
    }

    public function hash(#[SensitiveParameter] string $code): string
    {
        return hash_hmac('sha256', $this->normalise($code), $this->signingKey);
    }

    public function normalise(#[SensitiveParameter] string $code): string
    {
        return strtoupper(preg_replace('/[^A-Za-z0-9]/', '', $code) ?? '');
    }
}
