<?php

declare(strict_types=1);

namespace App\Handler\Workspace;

use App\Account\AccountRepository;
use App\Attribute\Route;
use App\Auth\AccountService;
use App\Enum\PermissionEnum;
use App\Enum\WorkspaceRole;
use App\Http\ApiException;
use App\Http\Json;
use App\Http\RouteOptions;
use App\Workspace\WorkspaceRepository;
use Psr\Http\Message\ResponseInterface;
use Psr\Http\Message\ServerRequestInterface;
use Psr\Http\Server\RequestHandlerInterface;

/**
 * Promotion to `owner` is gated on the target already holding a second factor. Refusing here —
 * rather than promoting and asking them to enrol later — is what keeps the invariant true at
 * every instant: there is never an owner without TOTP.
 */
final class UpdateMemberHandler implements RequestHandlerInterface
{
    public function __construct(
        private readonly WorkspaceRepository $workspaces,
        private readonly AccountRepository $accounts,
    ) {
    }

    #[Route(
        path: '/api/v1/workspaces/:workspace/members/:user',
        methods: ['PATCH'],
        name: 'workspaces.members.update',
        options: [RouteOptions::PERMISSION => PermissionEnum::WorkspaceManage],
    )]
    public function handle(ServerRequestInterface $request): ResponseInterface
    {
        $workspaceId = (string) $request->getAttribute(RouteOptions::WORKSPACE_PARAM);
        $targetId = (string) $request->getAttribute('user');

        $role = WorkspaceRole::tryFrom(Json::requireString(Json::body($request), 'role', 16));

        if ($role === null) {
            throw ApiException::unprocessable(
                'Role must be owner, editor or viewer.',
                ['field' => 'role'],
                'validation_failed'
            );
        }

        if ($this->workspaces->roleOf($targetId, $workspaceId) === null) {
            throw ApiException::notFound('That person is not a member of this workspace.', 'not_a_member');
        }

        if ($role === WorkspaceRole::Owner && $this->accounts->confirmedTotp($targetId) === null) {
            throw ApiException::conflict(
                'That member must enable two-factor authentication before they can be made an owner.',
                'totp_required_for_owner'
            );
        }

        $this->workspaces->setRole($workspaceId, $targetId, $role);

        return Json::ok(['members' => $this->workspaces->members($workspaceId)]);
    }
}
