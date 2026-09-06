<?php

declare(strict_types=1);

use App\Support\Env;

return [
    'storage' => [
        // MinIO locally; any S3-compatible provider in production.
        'endpoint'       => Env::string('S3_ENDPOINT'),
        'region'         => Env::string('S3_REGION', 'us-east-1'),
        'bucket'         => Env::string('S3_BUCKET', 'aurum-sheets'),
        'key'            => Env::string('S3_KEY', ''),
        'secret'         => Env::string('S3_SECRET', ''),
        'use_path_style' => Env::bool('S3_PATH_STYLE', true),
    ],
];
