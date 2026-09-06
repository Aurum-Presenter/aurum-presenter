<?php

declare(strict_types=1);

namespace AppTest\Invite;

use App\Enum\WorkspaceRole;
use App\Invite\InviteRepository;
use App\Support\Clock;
use App\Workspace\WorkspaceRepository;
use AppTest\ControlTestCase;

/**
 * Invitations: single-use, expiring, bound to the address they were sent to, and never stored
 * in a form that would let somebody read a database and walk into a band.
 */
final class InviteTest extends ControlTestCase
{
    private InviteRepository $invites;
    private WorkspaceRepository $workspaces;
    private string $ownerId;
    private string $workspaceId;

    protected function setUp(): void
    {
        parent::setUp();

        $clock = new Clock();
        $this->invites = new InviteRepository($this->control, $clock, 'test-signing-key');
        $this->workspaces = new WorkspaceRepository($this->control, $clock);

        $this->ownerId = $this->makeUser('owner@example.com', 'Kate');
        $this->workspaceId = $this->workspaces->create($this->ownerId, 'The Anchor');
    }

    public function testTheTokenIsNotStoredAndTheInviteIsFoundByIt(): void
    {
        $invite = $this->invites->create($this->workspaceId, 'bass@example.com', WorkspaceRole::Editor, $this->ownerId);

        $stored = $this->control->connection()->fetchOne('SELECT token_hash FROM invites WHERE id = ?', [$invite['id']]);

        self::assertNotSame($invite['token'], $stored, 'The raw token must never be stored.');
        self::assertSame($invite['id'], $this->invites->findByToken($invite['token'])['id']);
        self::assertNull($this->invites->findByToken('not-a-real-token'));
    }

    /** Resending is the common case, so a second invite replaces the first rather than colliding. */
    public function testInvitingTheSameAddressAgainReplacesTheFirstInvite(): void
    {
        $first = $this->invites->create($this->workspaceId, 'bass@example.com', WorkspaceRole::Editor, $this->ownerId);
        $second = $this->invites->create($this->workspaceId, 'bass@example.com', WorkspaceRole::Viewer, $this->ownerId);

        self::assertNull($this->invites->findByToken($first['token']));
        self::assertSame('viewer', $this->invites->findByToken($second['token'])['role']);
        self::assertCount(1, $this->invites->pending($this->workspaceId));
    }

    public function testAcceptingMarksItUsedAndItCannotBeUsedTwice(): void
    {
        $invite = $this->invites->create($this->workspaceId, 'bass@example.com', WorkspaceRole::Editor, $this->ownerId);

        $this->invites->accept($invite['id']);

        self::assertNotNull($this->invites->findByToken($invite['token'])['accepted_at']);
        self::assertSame([], $this->invites->pending($this->workspaceId));
    }

    public function testAnInviteExpires(): void
    {
        $invite = $this->invites->create($this->workspaceId, 'bass@example.com', WorkspaceRole::Editor, $this->ownerId);

        self::assertFalse($this->invites->isExpired($this->invites->findByToken($invite['token'])));

        $this->control->connection()->update(
            'invites',
            ['expires_at' => gmdate('Y-m-d\TH:i:s.v\Z', time() - 60)],
            ['id' => $invite['id']],
        );

        self::assertTrue($this->invites->isExpired($this->invites->findByToken($invite['token'])));
    }

    public function testRevokingStopsTheLinkWorking(): void
    {
        $invite = $this->invites->create($this->workspaceId, 'bass@example.com', WorkspaceRole::Editor, $this->ownerId);

        self::assertTrue($this->invites->revoke($this->workspaceId, $invite['id']));
        self::assertNull($this->invites->findByToken($invite['token']));
        self::assertFalse($this->invites->revoke($this->workspaceId, $invite['id']));
    }

    /** Business rule 2: a workspace always has an owner, on removal as well as on demotion. */
    public function testTheLastOwnerCannotBeRemoved(): void
    {
        $editorId = $this->makeUser('bass@example.com');
        $this->workspaces->addMember($this->workspaceId, $editorId, WorkspaceRole::Editor);

        $this->expectExceptionMessageMatches('/owner/i');
        $this->workspaces->assertNotLastOwner($this->workspaceId, $this->ownerId);
    }

    public function testAnOwnerCanBeRemovedWhenAnotherOwnerRemains(): void
    {
        $second = $this->makeUser('second@example.com');
        $this->workspaces->addMember($this->workspaceId, $second, WorkspaceRole::Owner);

        $this->workspaces->assertNotLastOwner($this->workspaceId, $this->ownerId);
        $this->workspaces->removeMember($this->workspaceId, $this->ownerId);

        self::assertNull($this->workspaces->roleOf($this->ownerId, $this->workspaceId));
        self::assertSame(WorkspaceRole::Owner, $this->workspaces->roleOf($second, $this->workspaceId));
    }
}
