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
use App\Storage\SheetKey;
use Psr\Http\Message\ResponseInterface;
use Psr\Http\Message\ServerRequestInterface;
use Psr\Http\Server\RequestHandlerInterface;

/**
 * A presigned upload for a workspace asset.
 *
 * Single-part rather than the sheets' resumable multipart: a slide background is a picture, not
 * a forty-page score, and the cap here says so. Bytes go straight to the object store, so the
 * API never handles them.
 */
final class UploadAssetHandler implements RequestHandlerInterface
{
    private const int MAX_SIZE = 16 * 1024 * 1024;
    private const int URL_TTL = 900;

    public function __construct(private readonly ObjectStore $store)
    {
    }

    #[Route(
        path: '/api/v1/workspaces/:workspace/assets/upload-url',
        methods: ['POST'],
        name: 'assets.upload-url',
        options: [RouteOptions::PERMISSION => PermissionEnum::WorkspaceWrite],
    )]
    public function handle(ServerRequestInterface $request): ResponseInterface
    {
        $workspaceId = (string) $request->getAttribute(RouteOptions::WORKSPACE_PARAM);
        $body = Json::body($request);

        $sha256 = strtolower(Json::requireString($body, 'sha256', 64));
        $contentType = strtolower(Json::requireString($body, 'content_type', 64));
        $size = (int) ($body['size'] ?? 0);

        if (! SheetKey::isValidSha256($sha256)) {
            throw ApiException::unprocessable('"sha256" must be 64 hex characters.', ['field' => 'sha256'], 'validation_failed');
        }

        if ($size <= 0 || $size > self::MAX_SIZE) {
            throw ApiException::unprocessable(
                sprintf('A background image must be between 1 and %d bytes.', self::MAX_SIZE),
                ['field' => 'size'],
                'validation_failed',
            );
        }

        $key = AssetKey::for($workspaceId, $sha256, $contentType);

        // Content addressing makes the "already uploaded" case free: identical bytes are here.
        if ($this->store->exists($key)) {
            return Json::ok(['already_stored' => true, 'key' => $key, 'asset' => $sha256]);
        }

        $upload = $this->store->createMultipartUpload($key, $contentType);

        return Json::ok([
            'already_stored' => false,
            'key'            => $key,
            'asset'          => $sha256,
            'upload_id'      => $upload['upload_id'],
            'url'            => $this->store->presignUploadPart($key, $upload['upload_id'], 1, self::URL_TTL),
            'expires_in'     => self::URL_TTL,
        ]);
    }
}
