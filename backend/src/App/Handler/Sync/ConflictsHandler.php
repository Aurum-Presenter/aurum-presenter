<?php

declare(strict_types=1);

namespace App\Handler\Sync;

use App\Attribute\Route;
use App\Enum\PermissionEnum;
use App\Http\Json;
use App\Http\RouteOptions;
use App\Middleware\WorkspaceMiddleware;
use Doctrine\DBAL\Connection;
use Psr\Http\Message\ResponseInterface;
use Psr\Http\Message\ServerRequestInterface;
use Psr\Http\Server\RequestHandlerInterface;

/**
 * The values that lost a per-field last-writer-wins resolution.
 *
 * They are kept server-side rather than only on the device that pushed, so the review panel
 * shows the same list on every device — the person who can judge which version was right is not
 * necessarily the person whose write lost.
 */
final class ConflictsHandler implements RequestHandlerInterface
{
    private const int LIMIT = 200;

    #[Route(
        path: '/api/v1/workspaces/:workspace/sync/conflicts',
        methods: ['GET'],
        name: 'sync.conflicts',
        options: [RouteOptions::PERMISSION => PermissionEnum::WorkspaceRead],
    )]
    public function handle(ServerRequestInterface $request): ResponseInterface
    {
        /** @var Connection $db */
        $db = $request->getAttribute(WorkspaceMiddleware::CONNECTION_ATTRIBUTE);
        $since = (string) ($request->getQueryParams()['since'] ?? '');

        $rows = $db->fetchAllAssociative(
            'SELECT id, table_name, record_id, field, losing_value, losing_user, at
             FROM sync_conflicts
             WHERE at > ?
             ORDER BY at DESC
             LIMIT ?',
            [$since, self::LIMIT],
        );

        return Json::ok(['conflicts' => $rows]);
    }
}
