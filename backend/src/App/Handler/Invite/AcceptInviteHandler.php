<?php

declare(strict_types=1);

namespace App\Handler\Invite;

use App\Attribute\Route;
use App\Enum\WorkspaceRole;
use App\Http\ApiException;
use App\Http\Json;
use App\Invite\InviteRepository;
use App\Middleware\SessionMiddleware;
use App\Workspace\WorkspaceRepository;
use Psr\Http\Message\ResponseInterface;
use Psr\Http\Message\ServerRequestInterface;
use Psr\Http\Server\RequestHandlerInterface;

/**
 * Accepting an invitation.
 *
 * Business rule 4: the invite binds to the address it was sent to. Accepting it while signed in
 * as somebody else is refused rather than quietly re-pointed, because an invite forwarded to a
 * colleague is exactly how a band ends up with a member nobody meant to add.
 */
final class AcceptInviteHandler implements RequestHandlerInterface
{
    public function __construct(
        private readonly InviteRepository $invites,
        private readonly WorkspaceRepository $workspaces,
    ) {
    }

    #[Route(path: '/api/v1/invites/accept', methods: ['POST'], name: 'invites.accept')]
    public function handle(ServerRequestInterface $request): ResponseInterface
    {
        $user = $request->getAttribute(SessionMiddleware::USER_ATTRIBUTE);
        $token = Json::requireString(Json::body($request), 'token', 128);
        $invite = $this->invites->findByToken($token);

        if ($invite === null) {
            throw ApiException::notFound('That invitation link is not valid.', 'invite_not_found');
        }

        if ($invite['accepted_at'] !== null) {
            throw ApiException::conflict('That invitation has already been used.', 'invite_used');
        }

        if ($this->invites->isExpired($invite)) {
            throw ApiException::conflict('That invitation has expired. Ask for a new one.', 'invite_expired');
        }

        if (strcasecmp((string) $invite['email'], (string) $user['email']) !== 0) {
            throw ApiException::forbidden(
                sprintf('That invitation was sent to %s. Sign in as that account to accept it.', (string) $invite['email']),
                'invite_wrong_account',
            );
        }

        $workspaceId = (string) $invite['workspace_id'];
        $role = WorkspaceRole::from((string) $invite['role']);

        if ($this->workspaces->roleOf((string) $user['id'], $workspaceId) === null) {
            $this->workspaces->addMember($workspaceId, (string) $user['id'], $role);
        }

        $this->invites->accept((string) $invite['id']);

        return Json::ok([
            'workspace' => $this->workspaces->find($workspaceId),
            'role'      => $role->value,
        ]);
    }
}
