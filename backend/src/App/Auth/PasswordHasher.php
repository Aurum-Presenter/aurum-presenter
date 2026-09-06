<?php

declare(strict_types=1);

namespace App\Auth;

use SensitiveParameter;

final class PasswordHasher
{
    /** @param array<string, int> $options */
    public function __construct(private readonly array $options = [])
    {
    }

    public function hash(#[SensitiveParameter] string $password): string
    {
        return password_hash($password, PASSWORD_ARGON2ID, $this->options);
    }

    public function verify(#[SensitiveParameter] string $password, string $hash): bool
    {
        return password_verify($password, $hash);
    }

    /**
     * True when the stored hash was produced with weaker parameters than the current policy.
     * Callers rehash on a *successful* sign-in, which is the only moment the plaintext exists.
     */
    public function needsRehash(string $hash): bool
    {
        return password_needs_rehash($hash, PASSWORD_ARGON2ID, $this->options);
    }
}
