<?php

declare(strict_types=1);

namespace App\Handler\Asset;

use App\Attribute\Route;
use App\Enum\PermissionEnum;
use App\Http\ApiException;
use App\Http\Json;
use App\Http\RouteOptions;
use App\Storage\AssetKey;
use App\Storage\ObjectStore;
use Psr\Http\Message\ResponseInterface;
use Psr\Http\Message\ServerRequestInterface;
use Psr\Http\Server\RequestHandlerInterface;

/**
 * Finishes an asset upload, and refuses one whose stored size does not match what was declared
 * — a content-addressed key must never point at bytes that are not what it says they are.
 */
final class CompleteAssetHandler implements RequestHandlerInterface
{
    public function __construct(private readonly ObjectStore $store)
    {
    }

    #[Route(
        path: '/api/v1/workspaces/:workspace/assets/complete',
        methods: ['POST'],
        name: 'assets.complete',
        options: [RouteOptions::PERMISSION => PermissionEnum::WorkspaceWrite],
    )]
    public function handle(ServerRequestInterface $request): ResponseInterface
    {
        $workspaceId = (string) $request->getAttribute(RouteOptions::WORKSPACE_PARAM);
        $body = Json::body($request);

        $sha256 = strtolower(Json::requireString($body, 'sha256', 64));
        $contentType = strtolower(Json::requireString($body, 'content_type', 64));
        $key = AssetKey::for($workspaceId, $sha256, $contentType);
        $uploadId = Json::optionalString($body, 'upload_id');

        if ($uploadId !== null) {
            try {
                $this->store->completeMultipartUpload($key, $uploadId, [[
                    'PartNumber' => 1,
                    'ETag'       => Json::requireString($body, 'etag', 128),
                ]]);
            } catch (\Throwable) {
                $this->store->abortMultipartUpload($key, $uploadId);

                throw ApiException::unprocessable('The upload could not be assembled. Start it again.', [], 'upload_incomplete');
            }
        }

        $head = $this->store->head($key);

        if ($head === null) {
            throw ApiException::unprocessable('No object was stored at that key.', [], 'upload_missing');
        }

        $declared = (int) ($body['size'] ?? 0);

        if ($declared > 0 && $head['size'] !== $declared) {
            $this->store->delete($key);

            throw ApiException::unprocessable(
                'The stored file does not match what was declared.',
                ['expected' => $declared, 'stored' => $head['size']],
                'checksum_mismatch',
            );
        }

        return Json::ok(['asset' => $sha256, 'content_type' => $contentType, 'size' => $head['size']]);
    }
}
