<?php

declare(strict_types=1);

namespace App\Handler\Invite;

use App\Attribute\Route;
use App\Enum\PermissionEnum;
use App\Enum\WorkspaceRole;
use App\Http\ApiException;
use App\Http\Json;
use App\Http\RouteOptions;
use App\Invite\InviteRepository;
use App\Mail\MailQueue;
use App\Middleware\SessionMiddleware;
use App\Support\Env;
use App\Workspace\WorkspaceRepository;
use Psr\Http\Message\ResponseInterface;
use Psr\Http\Message\ServerRequestInterface;
use Psr\Http\Server\RequestHandlerInterface;

/**
 * Inviting somebody to a workspace.
 *
 * The link is the credential, so it is shown to the inviter once and mailed once, and only its
 * hash is kept. An invite to `owner` is refused: ownership is granted after the person is in
 * and has a second factor, which is the invariant the member update handler protects.
 */
final class CreateInviteHandler implements RequestHandlerInterface
{
    public function __construct(
        private readonly InviteRepository $invites,
        private readonly WorkspaceRepository $workspaces,
        private readonly MailQueue $mail,
    ) {
    }

    #[Route(
        path: '/api/v1/workspaces/:workspace/invites',
        methods: ['POST'],
        name: 'invites.create',
        options: [RouteOptions::PERMISSION => PermissionEnum::WorkspaceManage],
    )]
    public function handle(ServerRequestInterface $request): ResponseInterface
    {
        $workspaceId = (string) $request->getAttribute(RouteOptions::WORKSPACE_PARAM);
        $user = $request->getAttribute(SessionMiddleware::USER_ATTRIBUTE);
        $body = Json::body($request);

        $email = strtolower(trim(Json::requireString($body, 'email', 320)));
        $role = WorkspaceRole::tryFrom(Json::requireString($body, 'role', 16));

        if (filter_var($email, FILTER_VALIDATE_EMAIL) === false) {
            throw ApiException::unprocessable('That is not an email address.', ['field' => 'email'], 'validation_failed');
        }

        if ($role === null || $role === WorkspaceRole::Owner) {
            throw ApiException::unprocessable(
                'Invite as editor or viewer; ownership is granted afterwards, once they have two-factor authentication.',
                ['field' => 'role'],
                'validation_failed',
            );
        }

        $workspace = $this->workspaces->find($workspaceId);
        $invite = $this->invites->create($workspaceId, $email, $role, (string) $user['id']);

        $link = rtrim(Env::string('APP_URL', 'http://localhost:5173'), '/') . '/invite/' . $invite['token'];
        $name = (string) ($workspace['name'] ?? 'a workspace');

        $this->mail->enqueue(
            $email,
            sprintf('%s invited you to %s on Aurum', (string) $user['display_name'], $name),
            sprintf(
                '<p>%s has invited you to join <strong>%s</strong> on Aurum Presenter.</p><p><a href="%s">Accept the invitation</a></p><p>The link works once and expires in %d days.</p>',
                htmlspecialchars((string) $user['display_name']),
                htmlspecialchars($name),
                htmlspecialchars($link),
                InviteRepository::TTL_DAYS,
            ),
            sprintf(
                "%s has invited you to join %s on Aurum Presenter.\n\n%s\n\nThe link works once and expires in %d days.\n",
                (string) $user['display_name'],
                $name,
                $link,
                InviteRepository::TTL_DAYS,
            ),
        );

        return Json::ok([
            'invite'  => ['id' => $invite['id'], 'email' => $email, 'role' => $role->value],
            // Returned so the inviter can hand it over directly — a band in a rehearsal room
            // should not have to wait for email to arrive.
            'link'    => $link,
            'pending' => $this->invites->pending($workspaceId),
        ], 201);
    }
}
