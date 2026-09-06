<?php

declare(strict_types=1);

namespace App\Handler\Workspace;

use App\Attribute\Route;
use App\Database\WorkspaceDatabase;
use App\Enum\PermissionEnum;
use App\Http\ApiException;
use App\Http\Json;
use App\Http\RouteOptions;
use App\Workspace\WorkspaceRepository;
use Psr\Http\Message\ResponseInterface;
use Psr\Http\Message\ServerRequestInterface;
use Psr\Http\Server\RequestHandlerInterface;

/**
 * Removing a member.
 *
 * Business rule 5: their membership goes and their per-user preferences for this workspace go
 * with it, because a preferred key is theirs and means nothing without them. Everything they
 * created — songs, charts, sets — stays, because it belongs to the band.
 */
final class RemoveMemberHandler implements RequestHandlerInterface
{
    public function __construct(
        private readonly WorkspaceRepository $workspaces,
        private readonly WorkspaceDatabase $databases,
    ) {
    }

    #[Route(
        path: '/api/v1/workspaces/:workspace/members/:user',
        methods: ['DELETE'],
        name: 'workspaces.members.remove',
        options: [RouteOptions::PERMISSION => PermissionEnum::WorkspaceManage],
    )]
    public function handle(ServerRequestInterface $request): ResponseInterface
    {
        $workspaceId = (string) $request->getAttribute(RouteOptions::WORKSPACE_PARAM);
        $targetId = (string) $request->getAttribute('user');

        if ($this->workspaces->roleOf($targetId, $workspaceId) === null) {
            throw ApiException::notFound('That person is not a member of this workspace.', 'not_a_member');
        }

        // A workspace always has an owner. Removing the last one would leave content nobody can
        // administer, so it is refused here as it is on a demotion.
        $this->workspaces->assertNotLastOwner($workspaceId, $targetId);
        $this->workspaces->removeMember($workspaceId, $targetId);

        $this->databases->open($workspaceId)->executeStatement(
            'DELETE FROM preferences WHERE user_id = ?',
            [$targetId],
        );

        return Json::ok(['members' => $this->workspaces->members($workspaceId)]);
    }
}
