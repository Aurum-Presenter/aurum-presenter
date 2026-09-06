<?php

declare(strict_types=1);

namespace App\Account;

use App\Database\ControlDatabase;
use App\Support\Clock;
use App\Support\Uuid;
use Doctrine\DBAL\Connection;

/**
 * Everything about who someone is, in control.sqlite. Knows nothing about workspaces.
 */
final class AccountRepository
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

    /** @return array<string, mixed>|null */
    public function findByEmail(string $email): ?array
    {
        $row = $this->db()->fetchAssociative('SELECT * FROM users WHERE email = ?', [$email]);

        return $row ?: null;
    }

    /** @return array<string, mixed>|null */
    public function findById(string $id): ?array
    {
        $row = $this->db()->fetchAssociative('SELECT * FROM users WHERE id = ?', [$id]);

        return $row ?: null;
    }

    public function create(string $email, string $displayName, string $passwordHash): string
    {
        $now = $this->clock->now();
        $id = Uuid::generate();

        $this->db()->transactional(function (Connection $db) use ($id, $email, $displayName, $passwordHash, $now): void {
            $db->insert('users', [
                'id'           => $id,
                'email'        => $email,
                'display_name' => $displayName,
                'created_at'   => $now,
                'updated_at'   => $now,
            ]);
            $db->insert('credentials', [
                'user_id'       => $id,
                'password_hash' => $passwordHash,
                'updated_at'    => $now,
            ]);
        });

        return $id;
    }

    public function passwordHash(string $userId): ?string
    {
        $hash = $this->db()->fetchOne('SELECT password_hash FROM credentials WHERE user_id = ?', [$userId]);

        return $hash === false ? null : (string) $hash;
    }

    public function updatePasswordHash(string $userId, string $hash): void
    {
        $this->db()->update(
            'credentials',
            ['password_hash' => $hash, 'updated_at' => $this->clock->now()],
            ['user_id' => $userId]
        );
    }

    // -- TOTP ------------------------------------------------------------------------------

    /** @return array<string, mixed>|null the row only once enrolment has been confirmed */
    public function confirmedTotp(string $userId): ?array
    {
        $row = $this->db()->fetchAssociative(
            'SELECT * FROM totp_secrets WHERE user_id = ? AND confirmed_at IS NOT NULL',
            [$userId]
        );

        return $row ?: null;
    }

    /** @return array<string, mixed>|null including unconfirmed enrolments in progress */
    public function pendingTotp(string $userId): ?array
    {
        $row = $this->db()->fetchAssociative('SELECT * FROM totp_secrets WHERE user_id = ?', [$userId]);

        return $row ?: null;
    }

    public function stageTotpSecret(string $userId, string $encryptedSecret): void
    {
        // Re-enrolling replaces any half-finished attempt; an abandoned enrolment must never
        // leave the account with a second factor it does not know about.
        $this->db()->delete('totp_secrets', ['user_id' => $userId]);
        $this->db()->insert('totp_secrets', [
            'user_id'          => $userId,
            'secret_encrypted' => $encryptedSecret,
            'created_at'       => $this->clock->now(),
        ]);
    }

    public function confirmTotp(string $userId, int $step): void
    {
        $this->db()->update(
            'totp_secrets',
            ['confirmed_at' => $this->clock->now(), 'last_accepted_step' => $step],
            ['user_id' => $userId]
        );
    }

    public function recordTotpStep(string $userId, int $step): void
    {
        $this->db()->update('totp_secrets', ['last_accepted_step' => $step], ['user_id' => $userId]);
    }

    public function removeTotp(string $userId): void
    {
        $this->db()->delete('totp_secrets', ['user_id' => $userId]);
        $this->db()->delete('recovery_codes', ['user_id' => $userId]);
    }

    // -- Recovery codes --------------------------------------------------------------------

    /** @param string[] $hashes */
    public function replaceRecoveryCodes(string $userId, array $hashes): void
    {
        $now = $this->clock->now();

        $this->db()->transactional(function (Connection $db) use ($userId, $hashes, $now): void {
            $db->delete('recovery_codes', ['user_id' => $userId]);

            foreach ($hashes as $hash) {
                $db->insert('recovery_codes', [
                    'id'         => Uuid::generate(),
                    'user_id'    => $userId,
                    'code_hash'  => $hash,
                    'created_at' => $now,
                ]);
            }
        });
    }

    public function consumeRecoveryCode(string $userId, string $hash): bool
    {
        $affected = $this->db()->executeStatement(
            'UPDATE recovery_codes SET used_at = ? WHERE user_id = ? AND code_hash = ? AND used_at IS NULL',
            [$this->clock->now(), $userId, $hash]
        );

        return $affected === 1;
    }

    public function remainingRecoveryCodes(string $userId): int
    {
        return (int) $this->db()->fetchOne(
            'SELECT COUNT(*) FROM recovery_codes WHERE user_id = ? AND used_at IS NULL',
            [$userId]
        );
    }

    // -- Two-factor challenges -------------------------------------------------------------

    public function createChallenge(string $userId, int $ttlSeconds): string
    {
        $id = Uuid::generate();

        $this->db()->insert('totp_challenges', [
            'id'         => $id,
            'user_id'    => $userId,
            'expires_at' => $this->clock->plusSeconds($ttlSeconds),
            'created_at' => $this->clock->now(),
        ]);

        return $id;
    }

    /** @return array<string, mixed>|null */
    public function findChallenge(string $id): ?array
    {
        $row = $this->db()->fetchAssociative('SELECT * FROM totp_challenges WHERE id = ?', [$id]);

        return $row ?: null;
    }

    public function countChallengeAttempt(string $id): int
    {
        $this->db()->executeStatement('UPDATE totp_challenges SET attempts = attempts + 1 WHERE id = ?', [$id]);

        return (int) $this->db()->fetchOne('SELECT attempts FROM totp_challenges WHERE id = ?', [$id]);
    }

    public function deleteChallenge(string $id): void
    {
        $this->db()->delete('totp_challenges', ['id' => $id]);
    }

    // -- Rate limiting ---------------------------------------------------------------------

    public function recordLoginAttempt(string $email, ?string $ip, bool $successful): void
    {
        $this->db()->insert('login_attempts', [
            'id'         => Uuid::generate(),
            'email'      => $email,
            'ip'         => $ip,
            'successful' => $successful ? 1 : 0,
            'at'         => $this->clock->now(),
        ]);
    }

    public function recentFailures(string $email, ?string $ip, int $withinSeconds): int
    {
        $since = $this->clock->minusSeconds($withinSeconds);

        return (int) $this->db()->fetchOne(
            'SELECT COUNT(*) FROM login_attempts
             WHERE successful = 0 AND at > ? AND (email = ? OR (ip IS NOT NULL AND ip = ?))',
            [$since, $email, $ip]
        );
    }

    public function clearFailures(string $email): void
    {
        $this->db()->executeStatement('DELETE FROM login_attempts WHERE email = ? AND successful = 0', [$email]);
    }
}
