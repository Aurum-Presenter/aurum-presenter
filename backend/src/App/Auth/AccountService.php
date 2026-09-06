<?php

declare(strict_types=1);

namespace App\Auth;

use App\Account\AccountRepository;
use App\Database\WorkspaceDatabase;
use App\Enum\WorkspaceRole;
use App\Http\ApiException;
use App\Workspace\WorkspaceRepository;
use SensitiveParameter;

/**
 * Registration and sign-in policy, kept out of the handlers so the rules are in one readable
 * place rather than spread across five endpoints.
 */
final class AccountService
{
    public function __construct(
        private readonly AccountRepository $accounts,
        private readonly WorkspaceRepository $workspaces,
        private readonly WorkspaceDatabase $databases,
        private readonly PasswordHasher $hasher,
        private readonly int $minPasswordLength = 12,
        private readonly int $lockoutThreshold = 5,
        private readonly int $lockoutWindowSeconds = 900,
    ) {
    }

    public function register(string $email, string $displayName, #[SensitiveParameter] string $password): string
    {
        $email = mb_strtolower(trim($email));

        if (! filter_var($email, FILTER_VALIDATE_EMAIL)) {
            throw ApiException::unprocessable('That does not look like an email address.', ['field' => 'email']);
        }

        $this->assertPasswordAcceptable($password);

        if ($this->accounts->findByEmail($email) !== null) {
            // Registration is one of the few places where disclosing existence is unavoidable,
            // since the account cannot be created either way. It is rate limited instead.
            throw ApiException::conflict('An account with that email already exists.', 'email_taken');
        }

        $userId = $this->accounts->create($email, $displayName, $this->hasher->hash($password));

        // Business rule 1: exactly one personal workspace, created on first sign-in and not
        // deletable. Creating the database file here means a brand-new account can write
        // offline immediately after its first sync.
        $workspaceId = $this->workspaces->create($userId, 'My songs', 'personal');
        $this->databases->open($workspaceId);

        return $userId;
    }

    /**
     * @return array<string, mixed> the user row
     * @throws ApiException on bad credentials, with the same message and timing either way
     */
    public function authenticate(string $email, #[SensitiveParameter] string $password, ?string $ip): array
    {
        $email = mb_strtolower(trim($email));

        $failures = $this->accounts->recentFailures($email, $ip, $this->lockoutWindowSeconds);

        if ($failures >= $this->lockoutThreshold) {
            // Escalating delay rather than a hard lock, so an attacker cannot lock a known
            // account out of its own sign-in by failing on purpose.
            $delay = min(2 ** ($failures - $this->lockoutThreshold), 8);
            usleep($delay * 250_000);

            if ($failures >= $this->lockoutThreshold * 4) {
                throw ApiException::tooManyRequests('Too many attempts. Try again shortly.', 60);
            }
        }

        $user = $this->accounts->findByEmail($email);
        $hash = $user === null ? null : $this->accounts->passwordHash((string) $user['id']);

        // Always run a verify, even with no account, so response time does not disclose whether
        // the email is registered.
        $reference = $hash ?? '$argon2id$v=19$m=65536,t=4,p=1$SFdrTUxlVFJqRG5wY0dGSw$0000000000000000000000000000000000000000000';
        $valid = $this->hasher->verify($password, $reference) && $user !== null;

        if (! $valid) {
            $this->accounts->recordLoginAttempt($email, $ip, false);

            throw ApiException::unauthorized('Email or password is incorrect.', 'invalid_credentials');
        }

        if ($hash !== null && $this->hasher->needsRehash($hash)) {
            // The only moment the plaintext exists under the current policy.
            $this->accounts->updatePasswordHash((string) $user['id'], $this->hasher->hash($password));
        }

        $this->accounts->recordLoginAttempt($email, $ip, true);
        $this->accounts->clearFailures($email);

        return $user;
    }

    /**
     * TOTP is mandatory for owners: an owner can remove members, transfer ownership and delete
     * the whole library, so the account that can do that must carry a second factor.
     */
    public function requiresTotp(string $userId): bool
    {
        foreach ($this->workspaces->forUser($userId) as $workspace) {
            if ($workspace['role'] === WorkspaceRole::Owner->value && $workspace['kind'] !== 'personal') {
                return true;
            }
        }

        return false;
    }

    public function assertPasswordAcceptable(#[SensitiveParameter] string $password): void
    {
        if (mb_strlen($password) < $this->minPasswordLength) {
            throw ApiException::unprocessable(
                sprintf('Password must be at least %d characters.', $this->minPasswordLength),
                ['field' => 'password']
            );
        }
    }
}
