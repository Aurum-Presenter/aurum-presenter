<?php

declare(strict_types=1);

namespace AppTest\Workspace;

use App\Enum\WorkspaceRole;
use App\Http\ApiException;
use App\Support\Clock;
use App\Workspace\WorkspaceRepository;
use AppTest\ControlTestCase;

/**
 * A workspace always has somebody who can administer it.
 *
 * The last owner can be neither demoted nor removed (acceptance criterion 5). Both paths go
 * through the same check, so a band cannot end up with songs, sets and sheets that nobody can
 * invite to, delete, or hand on.
 */
final class MembershipTest extends ControlTestCase
{
    private WorkspaceRepository $workspaces;
    private string $ownerId;
    private string $workspaceId;

    protected function setUp(): void
    {
        parent::setUp();

        $this->workspaces = new WorkspaceRepository($this->control, new Clock());
        $this->ownerId = $this->makeUser('kate@example.com');
        $this->workspaceId = $this->workspaces->create($this->ownerId, 'The Anchor');
    }

    public function testTheOnlyOwnerCannotStepDown(): void
    {
        $this->expectException(ApiException::class);
        $this->expectExceptionMessage('This workspace would be left without an owner.');

        $this->workspaces->setRole($this->workspaceId, $this->ownerId, WorkspaceRole::Editor);
    }

    public function testTheRoleIsUnchangedAfterTheAttempt(): void
    {
        try {
            $this->workspaces->setRole($this->workspaceId, $this->ownerId, WorkspaceRole::Viewer);
        } catch (ApiException) {
            // The point of the test is what is left behind.
        }

        self::assertSame(WorkspaceRole::Owner, $this->workspaces->roleOf($this->ownerId, $this->workspaceId));
    }

    public function testAnOwnerCanStepDownOnceThereIsAnother(): void
    {
        $second = $this->makeUser('sam@example.com');
        $this->workspaces->addMember($this->workspaceId, $second, WorkspaceRole::Owner);

        $this->workspaces->setRole($this->workspaceId, $this->ownerId, WorkspaceRole::Editor);

        self::assertSame(WorkspaceRole::Editor, $this->workspaces->roleOf($this->ownerId, $this->workspaceId));
        self::assertSame(WorkspaceRole::Owner, $this->workspaces->roleOf($second, $this->workspaceId));
    }

    public function testTheLastOwnerCannotBeRemovedEither(): void
    {
        $this->expectException(ApiException::class);
        $this->expectExceptionMessage('This workspace would be left without an owner.');

        $this->workspaces->assertNotLastOwner($this->workspaceId, $this->ownerId);
    }

    /** Demoting somebody who is not the last owner is ordinary, and must not be refused. */
    public function testAnEditorCanBeMadeAViewer(): void
    {
        $member = $this->makeUser('sam@example.com');
        $this->workspaces->addMember($this->workspaceId, $member, WorkspaceRole::Editor);

        $this->workspaces->setRole($this->workspaceId, $member, WorkspaceRole::Viewer);

        self::assertSame(WorkspaceRole::Viewer, $this->workspaces->roleOf($member, $this->workspaceId));
    }
}
