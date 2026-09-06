<?php

declare(strict_types=1);

use App\Support\Env;

$dataDir = Env::string('DATA_DIR', APP_DIR . '/var/data');

return [
    'database' => [
        // The identity tier. One file, opened on every request.
        'control_path' => Env::string('CONTROL_DB_PATH', $dataDir . '/control.sqlite'),

        // The content tier. One file per workspace, named by its UUID.
        'workspace_dir' => Env::string('WORKSPACE_DB_DIR', $dataDir . '/workspace'),

        'control_migrations'   => APP_DIR . '/migrations/control',
        'workspace_migrations' => APP_DIR . '/migrations/workspace',

        // PHP-FPM is multi-process, so concurrent writers must queue rather than fail.
        'busy_timeout_ms' => Env::int('SQLITE_BUSY_TIMEOUT_MS', 5000),

        // A workspace file behind the current migration set is brought up to date when it is
        // opened, so a file restored from an old backup repairs itself.
        'auto_migrate' => Env::bool('DB_AUTO_MIGRATE', true),
    ],
];
