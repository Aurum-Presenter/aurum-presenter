<?php

declare(strict_types=1);

namespace App\Handler\Sheet;

use App\Attribute\Route;
use App\Enum\PermissionEnum;
use App\Http\ApiException;
use App\Http\Json;
use App\Http\RouteOptions;
use App\Middleware\WorkspaceMiddleware;
use App\Storage\ObjectStore;
use App\Storage\SheetKey;
use Doctrine\DBAL\Connection;
use Psr\Http\Message\ResponseInterface;
use Psr\Http\Message\ServerRequestInterface;
use Psr\Http\Server\RequestHandlerInterface;

final class DownloadUrlHandler implements RequestHandlerInterface
{
    private const int URL_TTL = 3600;

    public function __construct(private readonly ObjectStore $store)
    {
    }

    #[Route(
        path: '/api/v1/workspaces/:workspace/sheets/:sheet/url',
        methods: ['GET'],
        name: 'sheets.url',
        options: [RouteOptions::PERMISSION => PermissionEnum::WorkspaceRead],
    )]
    public function handle(ServerRequestInterface $request): ResponseInterface
    {
        $workspaceId = (string) $request->getAttribute(RouteOptions::WORKSPACE_PARAM);
        $sheetId = (string) $request->getAttribute('sheet');

        /** @var Connection $db */
        $db = $request->getAttribute(WorkspaceMiddleware::CONNECTION_ATTRIBUTE);

        $row = $db->fetchAssociative('SELECT sha256, size FROM sheets WHERE id = ? AND deleted_at IS NULL', [$sheetId]);

        if ($row === false) {
            throw ApiException::notFound('No such sheet in this workspace.', 'sheet_not_found');
        }

        if ($row['sha256'] === null) {
            // A row without a file is the "not downloaded" state, not an error — the client
            // shows it plainly rather than treating it as a failure.
            throw ApiException::notFound('This sheet has no file yet.', 'file_not_uploaded');
        }

        return Json::ok([
            'url'        => $this->store->presignGet(SheetKey::for($workspaceId, (string) $row['sha256']), self::URL_TTL),
            'sha256'     => $row['sha256'],
            'size'       => $row['size'],
            'expires_in' => self::URL_TTL,
        ]);
    }
}
