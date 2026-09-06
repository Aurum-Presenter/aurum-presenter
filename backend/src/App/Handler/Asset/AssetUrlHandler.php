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

/** A presigned read for a workspace asset. Members only, like everything else in a workspace. */
final class AssetUrlHandler implements RequestHandlerInterface
{
    private const int URL_TTL = 3600;

    public function __construct(private readonly ObjectStore $store)
    {
    }

    #[Route(
        path: '/api/v1/workspaces/:workspace/assets/:asset/url',
        methods: ['GET'],
        name: 'assets.url',
        options: [RouteOptions::PERMISSION => PermissionEnum::WorkspaceRead],
    )]
    public function handle(ServerRequestInterface $request): ResponseInterface
    {
        $workspaceId = (string) $request->getAttribute(RouteOptions::WORKSPACE_PARAM);
        $sha256 = strtolower((string) $request->getAttribute('asset'));

        if (! SheetKey::isValidSha256($sha256)) {
            throw ApiException::notFound('No such asset.', 'asset_not_found');
        }

        // The extension is part of the key, so the type has to be named to find the object.
        foreach (AssetKey::contentTypes() as $contentType) {
            $key = AssetKey::for($workspaceId, $sha256, $contentType);

            if ($this->store->exists($key)) {
                return Json::ok([
                    'url'          => $this->store->presignGet($key, self::URL_TTL),
                    'content_type' => $contentType,
                    'expires_in'   => self::URL_TTL,
                ]);
            }
        }

        throw ApiException::notFound('That asset has not been uploaded.', 'asset_not_found');
    }
}
