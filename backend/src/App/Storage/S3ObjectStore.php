<?php

declare(strict_types=1);

namespace App\Storage;

use Aws\S3\S3Client;
use Aws\Exception\AwsException;

/**
 * S3-compatible storage. Path-style addressing is on by default so MinIO and other
 * self-hostable stores work without DNS tricks.
 *
 * Credentials never leave the server: the client receives presigned URLs with an expiry and
 * nothing else.
 */
final class S3ObjectStore implements ObjectStore
{
    public function __construct(
        private readonly S3Client $client,
        private readonly string $bucket,
    ) {
    }

    public function exists(string $key): bool
    {
        return $this->head($key) !== null;
    }

    public function head(string $key): ?array
    {
        try {
            $result = $this->client->headObject(['Bucket' => $this->bucket, 'Key' => $key]);
        } catch (AwsException) {
            return null;
        }

        return [
            'size'     => (int) $result['ContentLength'],
            'checksum' => $result['ChecksumSHA256'] ?? null,
        ];
    }

    public function presignGet(string $key, int $ttlSeconds): string
    {
        $command = $this->client->getCommand('GetObject', ['Bucket' => $this->bucket, 'Key' => $key]);

        return (string) $this->client->createPresignedRequest($command, sprintf('+%d seconds', $ttlSeconds))->getUri();
    }

    public function createMultipartUpload(string $key, string $contentType): array
    {
        $result = $this->client->createMultipartUpload([
            'Bucket'      => $this->bucket,
            'Key'         => $key,
            'ContentType' => $contentType,
        ]);

        return ['upload_id' => (string) $result['UploadId']];
    }

    public function presignUploadPart(string $key, string $uploadId, int $partNumber, int $ttlSeconds): string
    {
        $command = $this->client->getCommand('UploadPart', [
            'Bucket'     => $this->bucket,
            'Key'        => $key,
            'UploadId'   => $uploadId,
            'PartNumber' => $partNumber,
        ]);

        return (string) $this->client->createPresignedRequest($command, sprintf('+%d seconds', $ttlSeconds))->getUri();
    }

    public function completeMultipartUpload(string $key, string $uploadId, array $parts): void
    {
        $this->client->completeMultipartUpload([
            'Bucket'          => $this->bucket,
            'Key'             => $key,
            'UploadId'        => $uploadId,
            'MultipartUpload' => ['Parts' => $parts],
        ]);
    }

    public function abortMultipartUpload(string $key, string $uploadId): void
    {
        try {
            $this->client->abortMultipartUpload([
                'Bucket'   => $this->bucket,
                'Key'      => $key,
                'UploadId' => $uploadId,
            ]);
        } catch (AwsException) {
            // Aborting an upload that already completed or expired is not a failure worth
            // surfacing; the bucket lifecycle rule cleans up whatever is left.
        }
    }

    public function delete(string $key): void
    {
        $this->client->deleteObject(['Bucket' => $this->bucket, 'Key' => $key]);
    }

    public function listPrefix(string $prefix): array
    {
        $keys = [];
        $paginator = $this->client->getPaginator('ListObjectsV2', [
            'Bucket' => $this->bucket,
            'Prefix' => $prefix,
        ]);

        foreach ($paginator as $page) {
            foreach ($page['Contents'] ?? [] as $object) {
                $keys[] = (string) $object['Key'];
            }
        }

        return $keys;
    }
}
