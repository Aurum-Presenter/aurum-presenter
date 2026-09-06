<?php

declare(strict_types=1);

namespace App\Invite;

use App\Database\ControlDatabase;
use App\Enum\WorkspaceRole;
use App\Support\Clock;
use App\Support\Uuid;

/**
 * Invitations to a workspace.
 *
 * Only a hash of the token is stored, for the same reason only a hash of a password is: the
 * invite link is a bearer credential, and a copy of the database should not hand anybody a way
 * into somebody's band.
 */
final class InviteRepository
{
    /** Business rule 3: single-use, and dead after a fortnight. */
    public const int TTL_DAYS = 14;

    public function __construct(
        private readonly ControlDatabase $control,
        private readonly Clock $clock,
        private readonly string $signingKey,
    ) {
    }

    /** @return array{id: string, token: string} the token is returned once and never again */
    public function create(string $workspaceId, string $email, WorkspaceRole $role, string $invitedBy): array
    {
        $token = rtrim(strtr(base64_encode(random_bytes(32)), '+/', '-_'), '=');
        $id = Uuid::generate();

        // A second invite to the same address replaces the first rather than colliding with the
        // partial unique index — resending is the common case, not an error.
        $this->control->connection()->executeStatement(
            'DELETE FROM invites WHERE workspace_id = ? AND email = ? AND accepted_at IS NULL',
            [$workspaceId, $email],
        );

        $this->control->connection()->insert('invites', [
            'id'           => $id,
            'workspace_id' => $workspaceId,
            'email'        => $email,
            'role'         => $role->value,
            'token_hash'   => $this->hash($token),
            'invited_by'   => $invitedBy,
            'expires_at'   => $this->clock->plusSeconds(self::TTL_DAYS * 86400),
            'created_at'   => $this->clock->now(),
        ]);

        return ['id' => $id, 'token' => $token];
    }

    /** @return list<array<string, mixed>> */
    public function pending(string $workspaceId): array
    {
        return $this->control->connection()->fetchAllAssociative(
            'SELECT id, email, role, expires_at, created_at
             FROM invites
             WHERE workspace_id = ? AND accepted_at IS NULL
             ORDER BY created_at DESC',
            [$workspaceId],
        );
    }

    /** @return array<string, mixed>|null */
    public function findByToken(string $token): ?array
    {
        $row = $this->control->connection()->fetchAssociative(
            'SELECT i.*, w.name AS workspace_name
             FROM invites i
             JOIN workspaces w ON w.id = i.workspace_id
             WHERE i.token_hash = ?',
            [$this->hash($token)],
        );

        return $row === false ? null : $row;
    }

    public function accept(string $inviteId): void
    {
        $this->control->connection()->update(
            'invites',
            ['accepted_at' => $this->clock->now()],
            ['id' => $inviteId],
        );
    }

    public function revoke(string $workspaceId, string $inviteId): bool
    {
        return $this->control->connection()->executeStatement(
            'DELETE FROM invites WHERE id = ? AND workspace_id = ? AND accepted_at IS NULL',
            [$inviteId, $workspaceId],
        ) > 0;
    }

    /** @param array<string, mixed> $invite */
    public function isExpired(array $invite): bool
    {
        return $this->clock->isPast((string) $invite['expires_at']);
    }

    /**
     * Keyed rather than plain: a stolen database gives an attacker hashes, and without the
     * signing key those hashes cannot be checked against guessed tokens offline.
     */
    private function hash(string $token): string
    {
        return hash_hmac('sha256', $token, $this->signingKey);
    }
}
