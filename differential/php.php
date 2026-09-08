<?php

declare(strict_types=1);

/**
 * The third side of the differential harness: the PHP the server rules were ported from.
 *
 * The merge rules and the object keys have no TypeScript counterpart — they only ever existed
 * on the server — so they are compared here instead, and against the *real* SyncService driven
 * over a real SQLite workspace built from the real migrations. Nothing is stubbed: what this
 * reports is what the shipping server does.
 *
 * Reads `[{"rule": ..., "input": ...}, …]` on stdin, writes one canonical result per case.
 */

use App\Database\ConnectionFactory;
use App\Database\Migrator;
use App\Database\WriteTransaction;
use App\Enum\WorkspaceRole;
use App\Storage\AssetKey;
use App\Storage\OrphanSweep;
use App\Storage\SheetKey;
use App\Support\Clock;
use App\Sync\SyncSchema;
use App\Sync\SyncService;
use Doctrine\DBAL\Connection;

require __DIR__ . '/../backend/vendor/autoload.php';

$root = dirname(__DIR__);
$file = tempnam(sys_get_temp_dir(), 'aurum-diff-') . '.sqlite';

$factory = new ConnectionFactory();
$db = $factory->open($file);
(new Migrator($root . '/migrations/workspace'))->migrate($db);

$sync = new SyncService(new WriteTransaction(), new Clock());
$user = '01890000-0000-7000-8000-000000000001';

register_shutdown_function(static function () use ($file): void {
    foreach ([$file, $file . '-wal', $file . '-shm'] as $path) {
        @unlink($path);
    }
});

/** A UUIDv7-shaped id, derived from a counter so a rerun addresses the same rows. */
function id(int $number): string
{
    return sprintf('01890000-0000-7000-8000-%012d', $number);
}

function textOf(mixed $value): ?string
{
    if ($value === null) {
        return null;
    }

    if (is_bool($value)) {
        return $value ? '1' : '';
    }

    return is_scalar($value) ? (string) $value : json_encode($value, JSON_THROW_ON_ERROR);
}

/**
 * The observable outcome of one push: what the row looks like afterwards, and what was kept.
 *
 * Comparing the outcome rather than an internal decision is deliberate — the two
 * implementations are allowed to arrive there differently, and this is what a user would see.
 *
 * @param array<string, mixed> $input
 * @return array<string, mixed>
 */
function merge(Connection $db, SyncService $sync, string $user, array $input, int $number): array
{
    $table = is_string($input['table'] ?? null) ? $input['table'] : 'songs';
    $recordId = id($number);
    $columns = SyncSchema::columns($table);
    $existing = is_array($input['existing'] ?? null) ? $input['existing'] : null;

    // The one-default-arrangement rule reaches rows other than the one being written, so the
    // case needs a sibling to reach.
    $sibling = null;

    if ($table === 'arrangements' && is_string($input['song_id'] ?? null)) {
        $songId = $input['song_id'];
        $db->executeStatement(
            'INSERT OR IGNORE INTO songs (id, title, updated_at, change_seq) VALUES (?, ?, ?, 1)',
            [$songId, 'A Song', '2026-09-06T09:00:00.000Z'],
        );

        $sibling = id(800_000 + $number);
        $db->insert('arrangements', [
            'id'         => $sibling,
            'song_id'    => $songId,
            'name'       => 'Sibling',
            'body'       => '',
            'is_default' => 1,
            'updated_at' => '2026-09-06T09:00:00.000Z',
            'change_seq' => 1,
        ]);
    }

    if ($existing !== null) {
        $row = ['id' => $recordId];

        foreach ($columns as $column) {
            if (array_key_exists($column, $existing['fields'] ?? [])) {
                $row[$column] = textOf($existing['fields'][$column]);
            }
        }

        $db->insert($table, $row + [
            'updated_at' => $existing['updated_at'] ?? '2026-09-06T10:00:00.000Z',
            'change_seq' => 1,
            'deleted_at' => $existing['deleted_at'] ?? null,
            'updated_by' => $existing['updated_by'] ?? null,
        ]);
    }

    $op = [
        'op_id'     => id(900_000 + $number),
        'table'     => $table,
        'record_id' => $recordId,
        'op'        => ($input['kind'] ?? 'upsert') === 'delete' ? 'delete' : 'upsert',
        'payload'   => is_array($input['payload'] ?? null) ? $input['payload'] : [],
    ];

    if (is_string($input['base_updated_at'] ?? null)) {
        $op['base_updated_at'] = $input['base_updated_at'];
    }

    $result = $sync->push($db, [$op], $user, WorkspaceRole::Editor)['results'][0];

    if (($result['status'] ?? '') === 'rejected') {
        return ['rejected' => $result['code'] ?? 'unknown'];
    }

    $after = $db->fetchAssociative(sprintf('SELECT * FROM %s WHERE id = ?', $table), [$recordId]) ?: null;

    // Only the columns this case actually touched: the two implementations cannot be expected
    // to agree about a column default neither of them wrote.
    $touched = array_values(array_intersect(
        $columns,
        array_unique(array_merge(
            array_keys(is_array($input['payload'] ?? null) ? $input['payload'] : []),
            array_keys($existing['fields'] ?? []),
        )),
    ));
    sort($touched);

    $fields = [];
    foreach ($touched as $column) {
        $fields[$column] = $after === null ? null : textOf($after[$column] ?? null);
    }

    $conflicts = $db->fetchAllAssociative(
        'SELECT field, losing_value FROM sync_conflicts WHERE record_id = ? ORDER BY field',
        [$recordId],
    );

    return [
        'exists'    => $after !== null,
        'deleted'   => $after !== null && $after['deleted_at'] !== null,
        'fields'    => $fields,
        'conflicts' => array_map(
            static fn (array $row): array => [$row['field'], textOf($row['losing_value'])],
            $conflicts,
        ),
        'sibling_default' => $sibling === null
            ? null
            : textOf($db->fetchOne('SELECT is_default FROM arrangements WHERE id = ?', [$sibling])),
    ];
}

$cases = json_decode((string) file_get_contents('php://stdin'), true, 512, JSON_THROW_ON_ERROR);
$results = [];

foreach ($cases as $number => $case) {
    $rule = $case['rule'] ?? '';
    $input = is_array($case['input'] ?? null) ? $case['input'] : [];

    $results[] = match ($rule) {
        'sync_schema' => (static function () use ($input): array {
            $table = is_string($input['table'] ?? null) ? $input['table'] : '';
            $known = in_array($table, SyncSchema::tables(), true);

            return [
                'known'   => $known,
                'columns' => $known ? SyncSchema::columns($table) : null,
                'viewer'  => $known && SyncSchema::isViewerWritable($table),
                // Cast: an empty PHP array encodes as `[]`, and the shape has to be an object.
                'filtered' => (object) ($known
                    ? SyncSchema::filter($table, is_array($input['payload'] ?? null) ? $input['payload'] : [])
                    : []),
                'tables' => SyncSchema::tables(),
            ];
        })(),

        'object_keys' => (static function () use ($input): array {
            $workspace = (string) ($input['workspace'] ?? '');
            $hash = (string) ($input['sha256'] ?? '');
            $type = (string) ($input['content_type'] ?? '');
            $asset = null;

            try {
                $asset = AssetKey::for($workspace, $hash, $type);
            } catch (Throwable) {
                $asset = null;
            }

            return [
                'sheet'   => SheetKey::for($workspace, $hash),
                'valid'   => SheetKey::isValidSha256($hash),
                'asset'   => $asset,
                'orphans' => OrphanSweep::orphans(
                    array_map(
                        static fn (array $object): array => [
                            'key'      => (string) $object['key'],
                            'size'     => 0,
                            'modified' => (int) $object['modified'],
                        ],
                        is_array($input['objects'] ?? null) ? $input['objects'] : [],
                    ),
                    array_map(strval(...), is_array($input['referenced'] ?? null) ? $input['referenced'] : []),
                    (int) ($input['written_before'] ?? 0),
                ),
            ];
        })(),

        'merge' => merge($db, $sync, $user, $input, $number),

        default => ['skipped' => true],
    };
}

echo json_encode($results, JSON_THROW_ON_ERROR | JSON_UNESCAPED_SLASHES | JSON_UNESCAPED_UNICODE);
