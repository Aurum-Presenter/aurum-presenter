<?php

declare(strict_types=1);

use App\Support\Env;

return [
    'cors' => [
        // The refresh token is an httpOnly cookie, so credentials must be allowed — which in
        // turn forbids a wildcard origin. Origins are echoed only from this list.
        'allowed_origins' => array_values(array_filter(array_map(
            trim(...),
            explode(',', Env::string('CORS_ALLOWED_ORIGINS', 'http://localhost:5173,http://127.0.0.1:5173') ?? '')
        ))),
    ],
];
