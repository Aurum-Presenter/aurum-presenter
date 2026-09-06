<?php

declare(strict_types=1);

namespace App\Handler\Invite;

use App\Attribute\Route;
use App\Http\ApiException;
use App\Http\Json;
use App\Invite\InviteRepository;
use Psr\Http\Message\ResponseInterface;
use Psr\Http\Message\ServerRequestInterface;
use Psr\Http\Server\RequestHandlerInterface;

/**
 * What an invitation is for, before signing in.
 *
 * It says the workspace, the role and the address it was sent to — enough for somebody to know
 * which account to use — and nothing about the workspace's contents.
 */
final class PreviewInviteHandler implements RequestHandlerInterface
{
    public function __construct(private readonly InviteRepository $invites)
    {
    }

    #[Route(path: '/api/v1/invites/:token', methods: ['GET'], name: 'invites.preview')]
    public function handle(ServerRequestInterface $request): ResponseInterface
    {
        $invite = $this->invites->findByToken((string) $request->getAttribute('token'));

        if ($invite === null) {
            throw ApiException::notFound('That invitation link is not valid.', 'invite_not_found');
        }

        return Json::ok([
            'invite' => [
                'workspace_name' => $invite['workspace_name'],
                'email'          => $invite['email'],
                'role'           => $invite['role'],
                'expires_at'     => $invite['expires_at'],
                'used'           => $invite['accepted_at'] !== null,
                'expired'        => $this->invites->isExpired($invite),
            ],
        ]);
    }
}
