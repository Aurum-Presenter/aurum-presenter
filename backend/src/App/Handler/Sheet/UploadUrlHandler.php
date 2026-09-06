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

/**
 * Issues a resumable multipart upload as a set of presigned part URLs.
 *
 * Upload URLs live 15 minutes, not the hour a read URL gets: the blob queue uses them
 * immediately, and a leaked write URL is worse than a leaked read one.
 */
final class UploadUrlHandler implements RequestHandlerInterface
{
    private const int PART_SIZE = 8 * 1024 * 1024;
    private const int URL_TTL = 900;
    private const int MAX_SIZE = 512 * 1024 * 1024;

    public function __construct(private readonly ObjectStore $store)
    {
    }

    #[Route(
        path: '/api/v1/workspaces/:workspace/sheets/:sheet/upload-url',
        methods: ['POST'],
        name: 'sheets.upload-url',
        options: [RouteOptions::PERMISSION => PermissionEnum::WorkspaceWrite],
    )]
    public function handle(ServerRequestInterface $request): ResponseInterface
    {
        $workspaceId = (string) $request->getAttribute(RouteOptions::WORKSPACE_PARAM);
        $sheetId = (string) $request->getAttribute('sheet');
        $body = Json::body($request);

        $sha256 = strtolower(Json::requireString($body, 'sha256', 64));
        $size = (int) ($body['size'] ?? 0);

        if (! SheetKey::isValidSha256($sha256)) {
            throw ApiException::unprocessable('"sha256" must be 64 hex characters.', ['field' => 'sha256'], 'validation_failed');
        }

        if ($size <= 0 || $size > self::MAX_SIZE) {
            throw ApiException::unprocessable(
                sprintf('"size" must be between 1 and %d bytes.', self::MAX_SIZE),
                ['field' => 'size'],
                'validation_failed'
            );
        }

        /** @var Connection $db */
        $db = $request->getAttribute(WorkspaceMiddleware::CONNECTION_ATTRIBUTE);

        if ($db->fetchOne('SELECT 1 FROM sheets WHERE id = ?', [$sheetId]) === false) {
            throw ApiException::notFound('No such sheet in this workspace.', 'sheet_not_found');
        }

        $key = SheetKey::for($workspaceId, $sha256);

        // Content addressing makes deduplication free: identical bytes are already at this key.
        if ($this->store->exists($key)) {
            return Json::ok(['already_stored' => true, 'key' => $key]);
        }

        $upload = $this->store->createMultipartUpload($key, 'application/pdf');
        $partCount = (int) ceil($size / self::PART_SIZE);
        $parts = [];

        for ($number = 1; $number <= $partCount; $number++) {
            $parts[] = [
                'part_number' => $number,
                'offset'      => ($number - 1) * self::PART_SIZE,
                'length'      => min(self::PART_SIZE, $size - (($number - 1) * self::PART_SIZE)),
                'url'         => $this->store->presignUploadPart($key, $upload['upload_id'], $number, self::URL_TTL),
            ];
        }

        return Json::ok([
            'already_stored' => false,
            'key'            => $key,
            'upload_id'      => $upload['upload_id'],
            'part_size'      => self::PART_SIZE,
            'parts'          => $parts,
            'expires_in'     => self::URL_TTL,
        ]);
    }
}
