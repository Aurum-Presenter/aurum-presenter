<?php

declare(strict_types=1);

/**
 * One writer, for the concurrency test: open the workspace file the way the application does,
 * write a row inside an immediate transaction, exit non-zero if SQLite refused.
 */

require dirname(__DIR__, 2) . '/vendor/autoload.php';

use App\Database\ConnectionFactory;

$path = $argv[1] ?? '';
$writer = (int) ($argv[2] ?? 0);

try {
    $db = (new ConnectionFactory())->open($path);

    $db->executeStatement('BEGIN IMMEDIATE');
    $db->executeStatement('UPDATE sync_counter SET seq = seq + 1');
    $seq = (int) $db->fetchOne('SELECT seq FROM sync_counter');
    $db->insert('folders', [
        'id'         => sprintf('folder-%02d', $writer),
        'name'       => sprintf('Writer %d', $writer),
        'position'   => $writer,
        'updated_at' => '2026-09-06T00:00:00.000Z',
        'change_seq' => $seq,
    ]);
    $db->executeStatement('COMMIT');

    exit(0);
} catch (Throwable $error) {
    fwrite(STDERR, $error->getMessage());
    exit(1);
}
