<?php

declare(strict_types=1);

namespace App\Auth;

use App\Account\AccountRepository;
use App\Account\SessionRepository;
use App\Database\ControlDatabase;
use App\Support\Clock;
use App\Support\Uuid;
use SensitiveParameter;

/**
 * Forgotten passwords.
 *
 * A reset token is a bearer credential with a one-hour life, stored as a keyed hash and usable
 * once. Using it revokes every session the account has, because the reason somebody resets a
 * password is often that somebody else knows the old one.
 */
final class PasswordResetService
{
    public const int TTL_SECONDS = 3600;

    public function __construct(
        private readonly ControlDatabase $control,
        private readonly AccountRepository $accounts,
        private readonly SessionRepository $sessions,
        private readonly PasswordHasher $hasher,
        private readonly Clock $clock,
        private readonly string $signingKey,
    ) {
    }

    /** Returns null when the address has no account — the caller says the same thing either way. */
    public function start(string $email): ?string
    {
        $user = $this->accounts->findByEmail($email);

        if ($user === null) {
            return null;
        }

        $token = rtrim(strtr(base64_encode(random_bytes(32)), '+/', '-_'), '=');

        $this->control->connection()->insert('password_resets', [
            'id'         => Uuid::generate(),
            'user_id'    => (string) $user['id'],
            'token_hash' => $this->hash($token),
            'expires_at' => $this->clock->plusSeconds(self::TTL_SECONDS),
            'created_at' => $this->clock->now(),
        ]);

        return $token;
    }

    /** @return string|null the user id whose password was changed, or null if the token was no good */
    public function complete(string $token, #[SensitiveParameter] string $password): ?string
    {
        $row = $this->control->connection()->fetchAssociative(
            'SELECT * FROM password_resets WHERE token_hash = ?',
            [$this->hash($token)],
        );

        if ($row === false || $row['used_at'] !== null || $this->clock->isPast((string) $row['expires_at'])) {
            return null;
        }

        $userId = (string) $row['user_id'];

        $this->accounts->updatePasswordHash($userId, $this->hasher->hash($password));
        $this->control->connection()->update('password_resets', ['used_at' => $this->clock->now()], ['id' => $row['id']]);
        $this->sessions->revokeAllForUser($userId);

        return $userId;
    }

    private function hash(string $token): string
    {
        return hash_hmac('sha256', $token, $this->signingKey);
    }
}
