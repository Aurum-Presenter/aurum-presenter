<?php

declare(strict_types=1);

use App\Support\Env;

return [
    'auth' => [
        // Signs stateless access tokens and keys the refresh-token and recovery-code hashes.
        'signing_key' => Env::string('APP_SIGNING_KEY', 'insecure-development-key-change-me'),

        // 32 raw bytes, base64-encoded, for AES-256-GCM over the TOTP secrets.
        // Generate with: openssl rand -base64 32
        'secret_key' => Env::string('APP_SECRET_KEY', base64_encode(str_repeat('0', 32))),

        'access_ttl'  => Env::int('AUTH_ACCESS_TTL', 900),
        'refresh_ttl' => Env::int('AUTH_REFRESH_TTL', 2592000),

        'cookie_path'   => '/api/v1/auth',
        'cookie_secure' => Env::bool('AUTH_COOKIE_SECURE', true),

        'min_password_length' => 12,
        'lockout_threshold'   => 5,
        'lockout_window'      => 900,

        'totp_issuer' => Env::string('TOTP_ISSUER', 'Aurum Presenter'),

        'argon2' => [
            'memory_cost' => Env::int('ARGON2_MEMORY_COST', 65536),
            'time_cost'   => Env::int('ARGON2_TIME_COST', 4),
            'threads'     => Env::int('ARGON2_THREADS', 1),
        ],
    ],
];
