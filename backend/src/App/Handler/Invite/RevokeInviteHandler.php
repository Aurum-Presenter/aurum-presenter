<?php

declare(strict_types=1);

namespace App\Handler\Invite;

use App\Attribute\Route;
use App\Enum\PermissionEnum;
use App\Http\ApiException;
use App\Http\Json;
use App\Http\RouteOptions;
use App\Invite\InviteRepository;
use Psr\Http\Message\ResponseInterface;
use Psr\Http\Message\ServerRequestInterface;
use Psr\Http\Server\RequestHandlerInterface;

/** Withdrawing an invitation before it is used. The link stops working immediately. */
final class RevokeInviteHandler implements RequestHandlerInterface
{
    public function __construct(private readonly InviteRepository $invites)
    {
    }

    #[Route(
        path: '/api/v1/workspaces/:workspace/invites/:invite',
        methods: ['DELETE'],
        name: 'invites.revoke',
        options: [RouteOptions::PERMISSION => PermissionEnum::WorkspaceManage],
    )]
    public function handle(ServerRequestInterface $request): ResponseInterface
    {
        $workspaceId = (string) $request->getAttribute(RouteOptions::WORKSPACE_PARAM);
        $inviteId = (string) $request->getAttribute('invite');

        if (! $this->invites->revoke($workspaceId, $inviteId)) {
            throw ApiException::notFound('That invitation has already been used or withdrawn.', 'invite_not_found');
        }

        return Json::ok(['invites' => $this->invites->pending($workspaceId)]);
    }
}
