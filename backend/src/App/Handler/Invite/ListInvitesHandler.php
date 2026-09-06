<?php

declare(strict_types=1);

namespace App\Handler\Invite;

use App\Attribute\Route;
use App\Enum\PermissionEnum;
use App\Http\Json;
use App\Http\RouteOptions;
use App\Invite\InviteRepository;
use App\Mail\MailQueue;
use Psr\Http\Message\ResponseInterface;
use Psr\Http\Message\ServerRequestInterface;
use Psr\Http\Server\RequestHandlerInterface;

/** Pending invitations, plus anything the mailer gave up on, so "not sent" is visible. */
final class ListInvitesHandler implements RequestHandlerInterface
{
    public function __construct(
        private readonly InviteRepository $invites,
        private readonly MailQueue $mail,
    ) {
    }

    #[Route(
        path: '/api/v1/workspaces/:workspace/invites',
        methods: ['GET'],
        name: 'invites.list',
        options: [RouteOptions::PERMISSION => PermissionEnum::WorkspaceManage],
    )]
    public function handle(ServerRequestInterface $request): ResponseInterface
    {
        $workspaceId = (string) $request->getAttribute(RouteOptions::WORKSPACE_PARAM);
        $pending = $this->invites->pending($workspaceId);
        $stuck = array_column($this->mail->undelivered(), 'recipient');

        return Json::ok([
            'invites' => array_map(
                static fn (array $invite): array => $invite + ['not_sent' => in_array($invite['email'], $stuck, true)],
                $pending,
            ),
        ]);
    }
}
