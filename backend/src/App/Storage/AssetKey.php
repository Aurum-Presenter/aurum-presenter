<?php

declare(strict_types=1);

namespace App\Storage;

use App\Http\ApiException;

/**
 * Workspace assets — today, the background image behind an audience slide.
 *
 * Content-addressed under `assets/{workspace}/{sha256}.{ext}` for the same reasons sheets are:
 * uploading the same bytes twice writes one object, and replacing an image writes a new one
 * rather than changing what a device already cached under the old key.
 */
final class AssetKey
{
    /** What an audience screen can display, and nothing that a browser would execute. */
    private const array TYPES = [
        'image/png'  => 'png',
        'image/jpeg' => 'jpg',
        'image/webp' => 'webp',
        'image/avif' => 'avif',
    ];

    public static function for(string $workspaceId, string $sha256, string $contentType): string
    {
        return sprintf('assets/%s/%s.%s', $workspaceId, strtolower($sha256), self::extensionFor($contentType));
    }

    public static function prefixFor(string $workspaceId): string
    {
        return sprintf('assets/%s/', $workspaceId);
    }

    public static function extensionFor(string $contentType): string
    {
        $extension = self::TYPES[strtolower($contentType)] ?? null;

        if ($extension === null) {
            throw ApiException::unprocessable(
                'A background must be a PNG, JPEG, WebP or AVIF image.',
                ['field' => 'content_type'],
                'unsupported_type',
            );
        }

        return $extension;
    }

    /** @return list<string> */
    public static function contentTypes(): array
    {
        return array_keys(self::TYPES);
    }
}
