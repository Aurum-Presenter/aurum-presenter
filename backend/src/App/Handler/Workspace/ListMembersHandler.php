<?php

declare(strict_types=1);

namespace App\Handler\Workspace;

use App\Attribute\Route;
use App\Enum\PermissionEnum;
use App\Http\Json;
use App\Http\RouteOptions;
use App\Workspace\WorkspaceRepository;
use Psr\Http\Message\ResponseInterface;
use Psr\Http\Message\ServerRequestInterface;
use Psr\Http\Server\RequestHandlerInterface;

final class ListMembersHandler implements RequestHandlerInterface
{
    public function __construct(private readonly WorkspaceRepository $workspaces)
    {
    }

    #[Route(
        path: '/api/v1/workspaces/:workspace/members',
        methods: ['GET'],
        name: 'workspaces.members.list',
        options: [RouteOptions::PERMISSION => PermissionEnum::WorkspaceRead],
    )]
    public function handle(ServerRequestInterface $request): ResponseInterface
    {
        $workspaceId = (string) $request->getAttribute(RouteOptions::WORKSPACE_PARAM);

        return Json::ok([
            'members' => $this->workspaces->members($workspaceId),
            'invites' => $this->workspaces->pendingInvites($workspaceId),
        ]);
    }
}
