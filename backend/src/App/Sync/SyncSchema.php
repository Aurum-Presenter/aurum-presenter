<?php

declare(strict_types=1);

namespace App\Sync;

use App\Http\ApiException;

/**
 * The allowlist of synced tables and their client-writable columns.
 *
 * Nothing derived from the request is ever interpolated into SQL without passing through here
 * first. Column names cannot be bound as parameters, so an allowlist is the only safe way to
 * accept a client-supplied field set.
 */
final class SyncSchema
{
    /** Columns the server owns on every synced table. A client push may never set these. */
    public const array SYNC_COLUMNS = ['updated_at', 'change_seq', 'deleted_at', 'updated_by'];

    /** @var array<string, list<string>> */
    private const array TABLES = [
        'folders' => ['parent_id', 'name', 'position'],
        'songs' => [
            'folder_id', 'title', 'subtitle', 'authors', 'ccli_number', 'copyright', 'notes',
            'original_key', 'tempo', 'time_signature', 'tags',
        ],
        'arrangements' => ['song_id', 'name', 'body', 'default_key', 'is_default', 'position'],
        'sheets' => ['song_id', 'sheet_key', 'part', 'position', 'page_count', 'mime_type'],
        'annotations' => ['sheet_id', 'page', 'strokes', 'scope', 'author_id'],
        'sets' => ['name', 'scheduled_for', 'notes'],
        'set_items' => ['set_id', 'song_id', 'kind', 'title', 'key_override', 'position', 'notes'],
        'preferences' => ['user_id', 'scope_type', 'scope_id', 'name', 'value'],
    ];

    /**
     * Tables a `viewer` may write, because the rows are their own and no other member ever sees
     * them. Everything else needs workspace.write.
     */
    private const array VIEWER_WRITABLE = ['preferences', 'annotations'];

    /**
     * `sheets.sha256`, `size` and `uploaded_at` are set by the upload-completion endpoint after
     * the server has verified the stored object's checksum — never by a sync push, or a client
     * could claim a content hash it never uploaded.
     */
    public static function assertKnownTable(string $table): void
    {
        if (! isset(self::TABLES[$table])) {
            throw ApiException::unprocessable(
                sprintf('"%s" is not a synced table.', $table),
                ['table' => $table],
                'unknown_table'
            );
        }
    }

    /** @return list<string> */
    public static function columns(string $table): array
    {
        self::assertKnownTable($table);

        return self::TABLES[$table];
    }

    /** @return list<string> */
    public static function tables(): array
    {
        return array_keys(self::TABLES);
    }

    public static function isViewerWritable(string $table): bool
    {
        return in_array($table, self::VIEWER_WRITABLE, true);
    }

    /**
     * @param array<string, mixed> $payload
     * @return array<string, mixed> only the columns this table actually accepts
     */
    public static function filter(string $table, array $payload): array
    {
        $allowed = array_flip(self::columns($table));

        return array_intersect_key($payload, $allowed);
    }
}
