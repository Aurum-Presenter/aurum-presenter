<?php

declare(strict_types=1);

namespace App\Account;

use App\Database\ControlDatabase;
use App\Support\Clock;
use App\Support\Uuid;
use Doctrine\DBAL\Connection;

final class SessionRepository
{
    public function __construct(
        private readonly ControlDatabase $control,
        private readonly Clock $clock,
    ) {
    }

    private function db(): Connection
    {
        return $this->control->connection();
    }

    /**
     * @param string|null $familyId continues an existing rotation chain; null starts a new one
     * @return array{id: string, family_id: string}
     */
    public function open(string $userId, string $refreshTokenHash, string $expiresAt, ?string $userAgent, ?string $familyId = null): array
    {
        $id = Uuid::generate();
        $familyId ??= Uuid::generate();

        $this->db()->insert('sessions', [
            'id'                 => $id,
            'user_id'            => $userId,
            'family_id'          => $familyId,
            'refresh_token_hash' => $refreshTokenHash,
            'user_agent'         => $userAgent,
            'created_at'         => $this->clock->now(),
            'expires_at'         => $expiresAt,
        ]);

        return ['id' => $id, 'family_id' => $familyId];
    }

    /** @return array<string, mixed>|null */
    public function findByRefreshHash(string $hash): ?array
    {
        $row = $this->db()->fetchAssociative('SELECT * FROM sessions WHERE refresh_token_hash = ?', [$hash]);

        return $row ?: null;
    }

    /** @return array<string, mixed>|null */
    public function findById(string $id): ?array
    {
        $row = $this->db()->fetchAssociative('SELECT * FROM sessions WHERE id = ?', [$id]);

        return $row ?: null;
    }

    public function markReplaced(string $sessionId, string $replacementId): void
    {
        $this->db()->update(
            'sessions',
            ['revoked_at' => $this->clock->now(), 'replaced_by' => $replacementId],
            ['id' => $sessionId]
        );
    }

    /**
     * Reuse detection. A refresh token is single-use; seeing one again means a copy exists, and
     * there is no way to tell the thief from the victim — so the entire rotation chain dies and
     * both are made to sign in again.
     *
     * `replaced_by` is cleared as well as `revoked_at` set: a link in a rotation chain is
     * ordinarily still good enough to carry an access token to its expiry (see
     * `carriesAccessTokens`), and a chain that has been stolen must not be.
     */
    public function revokeFamily(string $familyId): void
    {
        $this->db()->executeStatement(
            'UPDATE sessions SET revoked_at = COALESCE(revoked_at, ?), replaced_by = NULL WHERE family_id = ?',
            [$this->clock->now(), $familyId]
        );
    }

    public function revoke(string $sessionId): void
    {
        $this->db()->executeStatement(
            'UPDATE sessions SET revoked_at = ?, replaced_by = NULL WHERE id = ? AND revoked_at IS NULL',
            [$this->clock->now(), $sessionId]
        );
    }

    public function revokeAllForUser(string $userId): void
    {
        $this->db()->executeStatement(
            'UPDATE sessions SET revoked_at = ?, replaced_by = NULL WHERE user_id = ? AND revoked_at IS NULL',
            [$this->clock->now(), $userId]
        );
    }

    /** Whether this session may still be *refreshed*: single-use, so a replaced one may not. */
    /** @param array<string, mixed> $session */
    public function isUsable(array $session): bool
    {
        return $session['revoked_at'] === null && ! $this->clock->isPast($session['expires_at']);
    }

    /**
     * Whether an access token already issued against this session is still honoured.
     *
     * Rotation replaces a session every time any window refreshes, and the app is routinely
     * open three times at once — library, control surface, stage. If a replaced session stopped
     * carrying its access token immediately, opening a stage window would knock the control
     * surface offline mid-service, which is precisely the moment it must not happen. A session
     * that was superseded keeps working until the token it issued expires, fifteen minutes at
     * most; one that was *revoked* — a sign-out, or a stolen chain — stops at once, because
     * revoking clears `replaced_by`.
     *
     * @param array<string, mixed> $session
     */
    public function carriesAccessTokens(array $session): bool
    {
        if ($this->clock->isPast($session['expires_at'])) {
            return false;
        }

        return $session['revoked_at'] === null || $session['replaced_by'] !== null;
    }

    public function purgeExpired(): int
    {
        return $this->db()->executeStatement('DELETE FROM sessions WHERE expires_at < ?', [$this->clock->now()]);
    }
}
