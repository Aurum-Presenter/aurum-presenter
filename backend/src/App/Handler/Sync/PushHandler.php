<?php

declare(strict_types=1);

namespace App\Handler\Sync;

use App\Attribute\Route;
use App\Enum\PermissionEnum;
use App\Enum\WorkspaceRole;
use App\Http\Json;
use App\Http\RouteOptions;
use App\Middleware\SessionMiddleware;
use App\Middleware\WorkspaceMiddleware;
use App\Sync\SyncService;
use Doctrine\DBAL\Connection;
use Psr\Http\Message\ResponseInterface;
use Psr\Http\Message\ServerRequestInterface;
use Psr\Http\Server\RequestHandlerInterface;

/**
 * The route requires only workspace.read, because a viewer is allowed to push their own
 * preferences and personal annotations. Per-table role enforcement is in SyncService, which is
 * the only place that knows which table each operation targets.
 */
final class PushHandler implements RequestHandlerInterface
{
    public function __construct(private readonly SyncService $sync)
    {
    }

    #[Route(
        path: '/api/v1/workspaces/:workspace/sync/push',
        methods: ['POST'],
        name: 'sync.push',
        options: [RouteOptions::PERMISSION => PermissionEnum::WorkspaceRead],
    )]
    public function handle(ServerRequestInterface $request): ResponseInterface
    {
        /** @var Connection $db */
        $db = $request->getAttribute(WorkspaceMiddleware::CONNECTION_ATTRIBUTE);
        /** @var WorkspaceRole $role */
        $role = $request->getAttribute(WorkspaceMiddleware::ROLE_ATTRIBUTE);
        $user = $request->getAttribute(SessionMiddleware::USER_ATTRIBUTE);

        $result = $this->sync->push(
            $db,
            array_values(Json::requireList(Json::body($request), 'ops')),
            (string) $user['id'],
            $role,
        );

        return Json::ok($result);
    }
}
