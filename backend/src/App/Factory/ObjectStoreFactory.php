<?php

declare(strict_types=1);

namespace App\Factory;

use App\Storage\ObjectStore;
use App\Storage\S3ObjectStore;
use Aws\S3\S3Client;
use Psr\Container\ContainerInterface;

final class ObjectStoreFactory
{
    public function __invoke(ContainerInterface $container): ObjectStore
    {
        $config = $container->get('config')['storage'] ?? [];

        $client = new S3Client(array_filter([
            'version'                 => 'latest',
            'region'                  => $config['region'] ?? 'us-east-1',
            'endpoint'                => $config['endpoint'] ?? null,
            // MinIO and most self-hostable stores address buckets by path, not by subdomain.
            'use_path_style_endpoint' => (bool) ($config['use_path_style'] ?? true),
            'credentials'             => [
                'key'    => $config['key'] ?? '',
                'secret' => $config['secret'] ?? '',
            ],
        ], static fn ($v) => $v !== null));

        return new S3ObjectStore($client, $config['bucket'] ?? 'aurum-sheets');
    }
}
