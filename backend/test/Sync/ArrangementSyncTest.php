<?php

declare(strict_types=1);

namespace AppTest\Sync;

use App\Database\WriteTransaction;
use App\Enum\WorkspaceRole;
use App\Support\Clock;
use App\Support\Uuid;
use App\Sync\SyncSchema;
use App\Sync\SyncService;
use AppTest\WorkspaceTestCase;

/**
 * The chart columns the chord-chart feature added: what a chart was pasted as, what it was
 * pasted from, and the arranger's capo. The server does nothing with any of them — parsing and
 * transposition are entirely client-side — so the only thing worth testing is that they
 * replicate, and that the body survives a round trip byte for byte.
 */
final class ArrangementSyncTest extends WorkspaceTestCase
{
    private SyncService $sync;
    private string $userId;

    protected function setUp(): void
    {
        parent::setUp();

        $this->sync = new SyncService(new WriteTransaction(), new Clock());
        $this->userId = Uuid::generate();
    }

    public function testChartColumnsReplicate(): void
    {
        $songId = Uuid::generate();
        $arrangementId = Uuid::generate();
        $body = "{verse: 1}\n[G]Amazing [G/B]grace how [C]sweet the [G]sound\n";

        $push = $this->sync->push($this->db, [
            [
                'op_id'     => Uuid::generate(),
                'table'     => 'songs',
                'record_id' => $songId,
                'op'        => 'upsert',
                'payload'   => ['title' => 'Amazing Grace', 'original_key' => 'G'],
            ],
            [
                'op_id'     => Uuid::generate(),
                'table'     => 'arrangements',
                'record_id' => $arrangementId,
                'op'        => 'upsert',
                'payload'   => [
                    'song_id'         => $songId,
                    'name'            => 'Default',
                    'body'            => $body,
                    'is_default'      => 1,
                    'default_key'     => 'G',
                    'source_notation' => 'over_lyrics',
                    'source_text'     => "G           C\nAmazing grace how sweet the sound\n",
                    'capo_hint'       => 2,
                ],
            ],
        ], $this->userId, WorkspaceRole::Editor);

        self::assertSame(['applied', 'applied'], array_column($push['results'], 'status'));

        $row = $this->sync->pull($this->db, 0, $this->userId)['tables']['arrangements'][0];

        self::assertSame($body, $row['body'], 'The stored ChordPro must survive a round trip unchanged.');
        self::assertSame('over_lyrics', $row['source_notation']);
        self::assertSame(2, (int) $row['capo_hint']);
        self::assertStringContainsString('Amazing grace', (string) $row['source_text']);
    }

    /** A chart pushed without the new columns is a ChordPro chart, which is the common case. */
    public function testNotationDefaultsToChordPro(): void
    {
        $songId = Uuid::generate();
        $arrangementId = Uuid::generate();

        $this->sync->push($this->db, [
            [
                'op_id' => Uuid::generate(), 'table' => 'songs', 'record_id' => $songId,
                'op' => 'upsert', 'payload' => ['title' => 'Untitled'],
            ],
            [
                'op_id' => Uuid::generate(), 'table' => 'arrangements', 'record_id' => $arrangementId,
                'op' => 'upsert', 'payload' => ['song_id' => $songId, 'name' => 'Default', 'body' => ''],
            ],
        ], $this->userId, WorkspaceRole::Editor);

        $row = $this->sync->pull($this->db, 0, $this->userId)['tables']['arrangements'][0];

        self::assertSame('chordpro', $row['source_notation']);
        self::assertNull($row['capo_hint']);
    }

    /**
     * Every client-writable column has to exist in the file, or a push naming it fails at the
     * database rather than at the allowlist — which is a 500, not a rejected operation.
     */
    public function testEveryAllowlistedColumnExistsInTheSchema(): void
    {
        foreach (SyncSchema::tables() as $table) {
            $columns = $this->db->fetchFirstColumn(
                sprintf('SELECT name FROM pragma_table_info(%s)', $this->db->quote($table))
            );

            foreach (SyncSchema::columns($table) as $column) {
                self::assertContains($column, $columns, sprintf('%s.%s is allowlisted but does not exist.', $table, $column));
            }
        }
    }
}
