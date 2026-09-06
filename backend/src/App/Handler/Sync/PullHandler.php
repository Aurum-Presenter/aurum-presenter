<?php

declare(strict_types=1);

namespace App\Handler\Sync;

use App\Attribute\Route;
use App\Enum\PermissionEnum;
use App\Http\Json;
use App\Http\RouteOptions;
use App\Middleware\WorkspaceMiddleware;
use App\Sync\SyncService;
use Doctrine\DBAL\Connection;
use Psr\Http\Message\ResponseInterface;
use Psr\Http\Message\ServerRequestInterface;
use Psr\Http\Server\RequestHandlerInterface;

/**
 * Delta pull by change sequence, never by wall-clock time — a device with a skewed clock still
 * converges.
 */
final class PullHandler implements RequestHandlerInterface
{
    public function __construct(private readonly SyncService $sync)
    {
    }

    #[Route(
        path: '/api/v1/workspaces/:workspace/sync/pull',
        methods: ['GET'],
        name: 'sync.pull',
        options: [RouteOptions::PERMISSION => PermissionEnum::WorkspaceRead],
    )]
    public function handle(ServerRequestInterface $request): ResponseInterface
    {
        /** @var Connection $db */
        $db = $request->getAttribute(WorkspaceMiddleware::CONNECTION_ATTRIBUTE);
        $query = $request->getQueryParams();

        $tables = isset($query['tables']) && is_string($query['tables']) && $query['tables'] !== ''
            ? array_map(trim(...), explode(',', $query['tables']))
            : null;

        return Json::ok($this->sync->pull(
            $db,
            (int) ($query['since'] ?? 0),
            $tables,
            min(max((int) ($query['limit'] ?? 1000), 1), 5000),
        ));
    }
}
