<?php

declare(strict_types=1);

return [
    'discovery' => [
        'discovery_locations' => [
            [
                'namespace' => 'App\\',
                'path' => 'src/App/',
            ],
        ],
        'discovery_cache' => ! in_array(getenv('APP_ENV'), ['development', 'test'], true),
        'discovery_cache_path' => getenv('APP_ENV') === 'test'
            ? 'data/cache/discovery-cache-test.php'
            : 'data/cache/discovery-cache.php',
    ],
];
