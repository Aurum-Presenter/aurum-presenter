<?php

declare(strict_types=1);

namespace App\Storage;

/**
 * Sheet PDFs live outside SQLite. The interface exists so the S3 client is not wired directly
 * into handlers, and so a filesystem-backed implementation stays possible for self-hosting.
 */
interface ObjectStore
{
    public function exists(string $key): bool;

    /** @return array{size: int, checksum: string|null}|null */
    public function head(string $key): ?array;

    public function presignGet(string $key, int $ttlSeconds): string;

    /** @return array{upload_id: string} */
    public function createMultipartUpload(string $key, string $contentType): array;

    public function presignUploadPart(string $key, string $uploadId, int $partNumber, int $ttlSeconds): string;

    /** @param list<array{PartNumber: int, ETag: string}> $parts */
    public function completeMultipartUpload(string $key, string $uploadId, array $parts): void;

    public function abortMultipartUpload(string $key, string $uploadId): void;

    public function delete(string $key): void;

    /** @return list<string> */
    public function listPrefix(string $prefix): array;
}
