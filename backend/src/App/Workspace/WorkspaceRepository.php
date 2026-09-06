<?php

declare(strict_types=1);

namespace App\Workspace;

use App\Database\ControlDatabase;
use App\Enum\WorkspaceRole;
use App\Http\ApiException;
use App\Support\Clock;
use App\Support\Uuid;
use Doctrine\DBAL\Connection;

/**
 * Workspaces, memberships and invites — all in control.sqlite.
 *
 * This is the only place that can answer "may this user open that file", which is why it is
 * separate from the per-workspace databases it grants access to.
 */
final class WorkspaceRepository
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

    /** @return list<array<string, mixed>> */
    public function forUser(string $userId): array
    {
        return $this->db()->fetchAllAssociative(
            'SELECT w.id, w.name, w.kind, w.created_at, w.updated_at, m.role
             FROM workspaces w
             JOIN memberships m ON m.workspace_id = w.id
             WHERE m.user_id = ? AND w.deleted_at IS NULL
             ORDER BY w.kind = \'personal\' DESC, w.name COLLATE NOCASE',
            [$userId]
        );
    }

    /** @return array<string, mixed>|null */
    public function find(string $id): ?array
    {
        $row = $this->db()->fetchAssociative('SELECT * FROM workspaces WHERE id = ? AND deleted_at IS NULL', [$id]);

        return $row ?: null;
    }

    /**
     * The single membership lookup that gates every workspace request.
     */
    public function roleOf(string $userId, string $workspaceId): ?WorkspaceRole
    {
        $role = $this->db()->fetchOne(
            'SELECT m.role FROM memberships m
             JOIN workspaces w ON w.id = m.workspace_id
             WHERE m.user_id = ? AND m.workspace_id = ? AND w.deleted_at IS NULL',
            [$userId, $workspaceId]
        );

        return $role === false ? null : WorkspaceRole::from((string) $role);
    }

    /**
     * @param string|null $id a client-generated UUID, so a workspace created in local-only mode
     *                        keeps its identity when it is claimed and no re-download occurs
     */
    public function create(string $ownerId, string $name, string $kind = 'band', ?string $id = null): string
    {
        $id ??= Uuid::generate();
        $now = $this->clock->now();

        $this->db()->transactional(function (Connection $db) use ($id, $ownerId, $name, $kind, $now): void {
            $db->insert('workspaces', [
                'id'         => $id,
                'name'       => $name,
                'kind'       => $kind,
                'created_at' => $now,
                'updated_at' => $now,
            ]);
            $db->insert('memberships', [
                'id'           => Uuid::generate(),
                'user_id'      => $ownerId,
                'workspace_id' => $id,
                'role'         => WorkspaceRole::Owner->value,
                'created_at'   => $now,
                'updated_at'   => $now,
            ]);
        });

        return $id;
    }

    public function personalWorkspaceId(string $userId): ?string
    {
        $id = $this->db()->fetchOne(
            'SELECT w.id FROM workspaces w
             JOIN memberships m ON m.workspace_id = w.id
             WHERE m.user_id = ? AND w.kind = \'personal\' AND w.deleted_at IS NULL',
            [$userId]
        );

        return $id === false ? null : (string) $id;
    }

    /** @return list<array<string, mixed>> */
    public function members(string $workspaceId): array
    {
        return $this->db()->fetchAllAssociative(
            'SELECT m.id, m.user_id, m.role, m.created_at, u.email, u.display_name
             FROM memberships m
             JOIN users u ON u.id = m.user_id
             WHERE m.workspace_id = ?
             ORDER BY u.display_name COLLATE NOCASE',
            [$workspaceId]
        );
    }

    public function countOwners(string $workspaceId): int
    {
        return (int) $this->db()->fetchOne(
            'SELECT COUNT(*) FROM memberships WHERE workspace_id = ? AND role = ?',
            [$workspaceId, WorkspaceRole::Owner->value]
        );
    }

    /**
     * Business rule 2: a workspace always has at least one owner. Demoting or removing the last
     * one is refused rather than silently leaving the workspace unmanageable.
     */
    public function assertNotLastOwner(string $workspaceId, string $userId): void
    {
        $current = $this->roleOf($userId, $workspaceId);

        if ($current === WorkspaceRole::Owner && $this->countOwners($workspaceId) <= 1) {
            throw ApiException::conflict(
                'This workspace would be left without an owner. Promote another member first.',
                'last_owner'
            );
        }
    }

    public function setRole(string $workspaceId, string $userId, WorkspaceRole $role): void
    {
        if ($role !== WorkspaceRole::Owner) {
            $this->assertNotLastOwner($workspaceId, $userId);
        }

        $this->db()->update(
            'memberships',
            ['role' => $role->value, 'updated_at' => $this->clock->now()],
            ['workspace_id' => $workspaceId, 'user_id' => $userId]
        );
    }

    public function addMember(string $workspaceId, string $userId, WorkspaceRole $role): void
    {
        $now = $this->clock->now();

        $this->db()->executeStatement(
            'INSERT INTO memberships (id, user_id, workspace_id, role, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?)
             ON CONFLICT (user_id, workspace_id) DO UPDATE SET role = excluded.role, updated_at = excluded.updated_at',
            [Uuid::generate(), $userId, $workspaceId, $role->value, $now, $now]
        );
    }

    public function removeMember(string $workspaceId, string $userId): void
    {
        $this->assertNotLastOwner($workspaceId, $userId);
        $this->db()->delete('memberships', ['workspace_id' => $workspaceId, 'user_id' => $userId]);
    }

    // -- Invites ---------------------------------------------------------------------------

    public function createInvite(
        string $workspaceId,
        string $email,
        WorkspaceRole $role,
        string $tokenHash,
        string $invitedBy,
        int $ttlDays = 14,
    ): string {
        $id = Uuid::generate();

        $this->db()->executeStatement(
            'DELETE FROM invites WHERE workspace_id = ? AND email = ? AND accepted_at IS NULL',
            [$workspaceId, $email]
        );

        $this->db()->insert('invites', [
            'id'           => $id,
            'workspace_id' => $workspaceId,
            'email'        => $email,
            'role'         => $role->value,
            'token_hash'   => $tokenHash,
            'invited_by'   => $invitedBy,
            'expires_at'   => $this->clock->plusSeconds($ttlDays * 86400),
            'created_at'   => $this->clock->now(),
        ]);

        return $id;
    }

    /** @return array<string, mixed>|null */
    public function findInviteByTokenHash(string $tokenHash): ?array
    {
        $row = $this->db()->fetchAssociative('SELECT * FROM invites WHERE token_hash = ?', [$tokenHash]);

        return $row ?: null;
    }

    public function markInviteAccepted(string $inviteId): void
    {
        $this->db()->update('invites', ['accepted_at' => $this->clock->now()], ['id' => $inviteId]);
    }

    /** @return list<array<string, mixed>> */
    public function pendingInvites(string $workspaceId): array
    {
        return $this->db()->fetchAllAssociative(
            'SELECT id, email, role, expires_at, created_at FROM invites
             WHERE workspace_id = ? AND accepted_at IS NULL ORDER BY created_at DESC',
            [$workspaceId]
        );
    }
}
