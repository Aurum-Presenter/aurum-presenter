<?php

declare(strict_types=1);

namespace App\Handler\Sheet;

use App\Attribute\Route;
use App\Database\WriteTransaction;
use App\Enum\PermissionEnum;
use App\Http\ApiException;
use App\Http\Json;
use App\Http\RouteOptions;
use App\Middleware\SessionMiddleware;
use App\Middleware\WorkspaceMiddleware;
use App\Storage\ObjectStore;
use App\Storage\SheetKey;
use App\Support\Clock;
use Doctrine\DBAL\Connection;
use Psr\Http\Message\ResponseInterface;
use Psr\Http\Message\ServerRequestInterface;
use Psr\Http\Server\RequestHandlerInterface;

/**
 * Finalises the upload and links it to the sheet row.
 *
 * The size the store reports is checked against what the client declared. Without a check of
 * some kind, a content-addressed key could be made to point at bytes that do not hash to it —
 * which would poison every future deduplication against that hash.
 */
final class CompleteUploadHandler implements RequestHandlerInterface
{
    public function __construct(
        private readonly ObjectStore $store,
        private readonly WriteTransaction $transaction,
        private readonly Clock $clock,
    ) {
    }

    #[Route(
        path: '/api/v1/workspaces/:workspace/sheets/:sheet/complete',
        methods: ['POST'],
        name: 'sheets.complete',
        options: [RouteOptions::PERMISSION => PermissionEnum::WorkspaceWrite],
    )]
    public function handle(ServerRequestInterface $request): ResponseInterface
    {
        $workspaceId = (string) $request->getAttribute(RouteOptions::WORKSPACE_PARAM);
        $sheetId = (string) $request->getAttribute('sheet');
        $user = $request->getAttribute(SessionMiddleware::USER_ATTRIBUTE);
        $body = Json::body($request);

        $sha256 = strtolower(Json::requireString($body, 'sha256', 64));

        if (! SheetKey::isValidSha256($sha256)) {
            throw ApiException::unprocessable('"sha256" must be 64 hex characters.', ['field' => 'sha256'], 'validation_failed');
        }

        $key = SheetKey::for($workspaceId, $sha256);
        $uploadId = Json::optionalString($body, 'upload_id');

        if ($uploadId !== null) {
            $parts = array_map(
                static fn (array $part): array => [
                    'PartNumber' => (int) $part['part_number'],
                    'ETag'       => (string) $part['etag'],
                ],
                array_values(Json::requireList($body, 'parts')),
            );

            if ($parts === []) {
                throw ApiException::unprocessable('"parts" cannot be empty.', ['field' => 'parts'], 'validation_failed');
            }

            try {
                $this->store->completeMultipartUpload($key, $uploadId, $parts);
            } catch (\Throwable $e) {
                $this->store->abortMultipartUpload($key, $uploadId);

                throw ApiException::unprocessable(
                    'The upload could not be assembled. Start it again.',
                    [],
                    'upload_incomplete'
                );
            }
        }

        $head = $this->store->head($key);

        if ($head === null) {
            throw ApiException::unprocessable('No object was stored at that key.', [], 'upload_missing');
        }

        $declaredSize = (int) ($body['size'] ?? 0);

        if ($declaredSize > 0 && $head['size'] !== $declaredSize) {
            $this->store->delete($key);

            throw ApiException::unprocessable(
                'The stored file does not match what was declared.',
                ['expected' => $declaredSize, 'stored' => $head['size']],
                'checksum_mismatch'
            );
        }

        /** @var Connection $db */
        $db = $request->getAttribute(WorkspaceMiddleware::CONNECTION_ATTRIBUTE);
        $pageCount = isset($body['page_count']) ? (int) $body['page_count'] : null;

        $updated = $this->transaction->run($db, function (Connection $db, int $seq) use ($sheetId, $sha256, $head, $pageCount, $user): int {
            return $db->update('sheets', array_filter([
                'sha256'      => $sha256,
                'size'        => $head['size'],
                'page_count'  => $pageCount,
                'uploaded_at' => $this->clock->now(),
                'updated_at'  => $this->clock->now(),
                'change_seq'  => $seq,
                'updated_by'  => (string) $user['id'],
            ], static fn ($v) => $v !== null), ['id' => $sheetId]);
        });

        if ($updated === 0) {
            throw ApiException::notFound('No such sheet in this workspace.', 'sheet_not_found');
        }

        return Json::ok(['sheet_id' => $sheetId, 'sha256' => $sha256, 'size' => $head['size']]);
    }
}
