<?php

declare(strict_types=1);

namespace App\Auth;

use App\Support\Clock;
use SensitiveParameter;

/**
 * Two token shapes, deliberately different:
 *
 *  - The **access token** is stateless and HMAC-signed, valid for minutes. Verifying it is a
 *    hash_equals, not a database read, so the sync engine's high-frequency calls do not each
 *    cost a row lookup.
 *  - The **refresh token** is a long opaque random string, stored only as a hash. A database
 *    read therefore cannot mint a session, and rotation on every use makes a stolen cookie
 *    detectable: presenting a token twice means someone has a copy, and the whole family dies.
 */
final class TokenService
{
    private const string ACCESS_PREFIX = 'v1';

    public function __construct(
        private readonly Clock $clock,
        private readonly string $signingKey,
        private readonly int $accessTtlSeconds = 900,
        private readonly int $refreshTtlSeconds = 2592000,
    ) {
    }

    public function issueAccessToken(string $userId, string $sessionId): string
    {
        $payload = $this->base64UrlEncode((string) json_encode([
            'sub' => $userId,
            'sid' => $sessionId,
            'exp' => time() + $this->accessTtlSeconds,
        ], JSON_THROW_ON_ERROR));

        return sprintf('%s.%s.%s', self::ACCESS_PREFIX, $payload, $this->sign($payload));
    }

    /**
     * @return array{sub: string, sid: string, exp: int}|null null when malformed, tampered
     *                                                        with, or expired
     */
    public function verifyAccessToken(#[SensitiveParameter] string $token): ?array
    {
        $parts = explode('.', $token);
        if (count($parts) !== 3 || $parts[0] !== self::ACCESS_PREFIX) {
            return null;
        }

        [, $payload, $signature] = $parts;

        if (! hash_equals($this->sign($payload), $signature)) {
            return null;
        }

        $decoded = json_decode($this->base64UrlDecode($payload), true);
        if (! is_array($decoded) || ! isset($decoded['sub'], $decoded['sid'], $decoded['exp'])) {
            return null;
        }

        if ((int) $decoded['exp'] <= time()) {
            return null;
        }

        return $decoded;
    }

    public function generateRefreshToken(): string
    {
        return bin2hex(random_bytes(32));
    }

    public function hashRefreshToken(#[SensitiveParameter] string $token): string
    {
        // A keyed hash, not a bare one: the stored value is useless to an attacker who reads the
        // database but not the application key.
        return hash_hmac('sha256', $token, $this->signingKey);
    }

    public function refreshExpiresAt(): string
    {
        return $this->clock->plusSeconds($this->refreshTtlSeconds);
    }

    public function accessTtlSeconds(): int
    {
        return $this->accessTtlSeconds;
    }

    public function refreshTtlSeconds(): int
    {
        return $this->refreshTtlSeconds;
    }

    private function sign(string $payload): string
    {
        return $this->base64UrlEncode(hash_hmac('sha256', $payload, $this->signingKey, true));
    }

    private function base64UrlEncode(string $value): string
    {
        return rtrim(strtr(base64_encode($value), '+/', '-_'), '=');
    }

    private function base64UrlDecode(string $value): string
    {
        return (string) base64_decode(strtr($value, '-_', '+/'), true);
    }
}
