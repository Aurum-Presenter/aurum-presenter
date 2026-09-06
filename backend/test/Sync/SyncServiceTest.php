<?php

declare(strict_types=1);

namespace AppTest\Sync;

use App\Database\WriteTransaction;
use App\Enum\WorkspaceRole;
use App\Support\Clock;
use App\Support\Uuid;
use App\Sync\SyncService;
use AppTest\WorkspaceTestCase;

final class SyncServiceTest extends WorkspaceTestCase
{
    private SyncService $sync;
    private string $userId;

    protected function setUp(): void
    {
        parent::setUp();

        $this->sync = new SyncService(new WriteTransaction(), new Clock());
        $this->userId = Uuid::generate();
    }

    /** @param array<string, mixed> $payload */
    private function op(string $table, string $recordId, array $payload, ?string $opId = null, string $kind = 'upsert', ?string $base = null): array
    {
        return array_filter([
            'op_id'           => $opId ?? Uuid::generate(),
            'table'           => $table,
            'record_id'       => $recordId,
            'op'              => $kind,
            'payload'         => $payload,
            'base_updated_at' => $base,
        ], static fn ($v) => $v !== null);
    }

    public function testPushInsertsAndPullReturnsTheRow(): void
    {
        $songId = Uuid::generate();

        $push = $this->sync->push(
            $this->db,
            [$this->op('songs', $songId, ['title' => 'Be Thou My Vision', 'original_key' => 'D'])],
            $this->userId,
            WorkspaceRole::Editor,
        );

        self::assertSame('applied', $push['results'][0]['status']);

        $pull = $this->sync->pull($this->db, 0, $this->userId);

        self::assertCount(1, $pull['tables']['songs']);
        self::assertSame('Be Thou My Vision', $pull['tables']['songs'][0]['title']);
    }

    /**
     * A retried batch after a lost response must not apply twice. This is the property that
     * lets the client retry blindly on a network error.
     */
    public function testReplayedOperationIsRecognisedAsADuplicate(): void
    {
        $songId = Uuid::generate();
        $opId = Uuid::generate();

        $this->sync->push($this->db, [$this->op('songs', $songId, ['title' => 'Original'], $opId)], $this->userId, WorkspaceRole::Editor);
        $second = $this->sync->push($this->db, [$this->op('songs', $songId, ['title' => 'Replayed'], $opId)], $this->userId, WorkspaceRole::Editor);

        self::assertSame('duplicate', $second['results'][0]['status']);
        self::assertSame('Original', $this->db->fetchOne('SELECT title FROM songs WHERE id = ?', [$songId]));
    }

    public function testPullByWatermarkReturnsOnlyNewerRows(): void
    {
        $first = Uuid::generate();
        $this->sync->push($this->db, [$this->op('songs', $first, ['title' => 'First'])], $this->userId, WorkspaceRole::Editor);

        $watermark = $this->sync->pull($this->db, 0, $this->userId)['change_seq'];

        $second = Uuid::generate();
        $this->sync->push($this->db, [$this->op('songs', $second, ['title' => 'Second'])], $this->userId, WorkspaceRole::Editor);

        $delta = $this->sync->pull($this->db, $watermark, $this->userId);

        self::assertCount(1, $delta['tables']['songs']);
        self::assertSame('Second', $delta['tables']['songs'][0]['title']);
    }

    /**
     * The losing value must survive. Last-writer-wins is acceptable only because the value it
     * displaces is recoverable afterwards.
     */
    public function testConcurrentEditRecordsTheDisplacedValue(): void
    {
        $songId = Uuid::generate();

        $this->sync->push($this->db, [$this->op('songs', $songId, ['title' => 'From device A'])], $this->userId, WorkspaceRole::Editor);

        // Device B edited from a base that predates device A's write.
        $stale = '2020-01-01T00:00:00.000Z';
        $push = $this->sync->push(
            $this->db,
            [$this->op('songs', $songId, ['title' => 'From device B'], null, 'upsert', $stale)],
            Uuid::generate(),
            WorkspaceRole::Editor,
        );

        self::assertSame(['title'], $push['results'][0]['conflicts']);
        self::assertSame('From device B', $this->db->fetchOne('SELECT title FROM songs WHERE id = ?', [$songId]));

        $conflict = $this->db->fetchAssociative('SELECT * FROM sync_conflicts WHERE record_id = ?', [$songId]);

        self::assertSame('title', $conflict['field']);
        self::assertSame('From device A', $conflict['losing_value'], 'The overwritten value must be recoverable.');
    }

    public function testTombstoneWinsOverALaterEdit(): void
    {
        $songId = Uuid::generate();

        $this->sync->push($this->db, [$this->op('songs', $songId, ['title' => 'Doomed'])], $this->userId, WorkspaceRole::Editor);
        $this->sync->push($this->db, [$this->op('songs', $songId, [], null, 'delete')], $this->userId, WorkspaceRole::Editor);
        $this->sync->push($this->db, [$this->op('songs', $songId, ['title' => 'Resurrected'])], $this->userId, WorkspaceRole::Editor);

        $row = $this->db->fetchAssociative('SELECT * FROM songs WHERE id = ?', [$songId]);

        self::assertNotNull($row['deleted_at'], 'A delete must not be undone by a stale edit.');
        self::assertSame('Doomed', $row['title']);
    }

    public function testViewerCannotWriteSharedContent(): void
    {
        $result = $this->sync->push(
            $this->db,
            [$this->op('songs', Uuid::generate(), ['title' => 'Should be refused'])],
            $this->userId,
            WorkspaceRole::Viewer,
        );

        self::assertSame('rejected', $result['results'][0]['status']);
        self::assertSame('insufficient_role', $result['results'][0]['code']);
        self::assertSame(0, (int) $this->db->fetchOne('SELECT COUNT(*) FROM songs'));
    }

    public function testViewerMayWriteTheirOwnPreferences(): void
    {
        $result = $this->sync->push(
            $this->db,
            [$this->op('preferences', Uuid::generate(), [
                'user_id'    => $this->userId,
                'scope_type' => 'song',
                'scope_id'   => Uuid::generate(),
                'name'       => 'preferred_key',
                'value'      => '"G"',
            ])],
            $this->userId,
            WorkspaceRole::Viewer,
        );

        self::assertSame('applied', $result['results'][0]['status']);
    }

    public function testViewerCannotWriteSomeoneElsesPreferences(): void
    {
        $result = $this->sync->push(
            $this->db,
            [$this->op('preferences', Uuid::generate(), [
                'user_id'    => Uuid::generate(),
                'scope_type' => 'workspace',
                'name'       => 'font_size',
                'value'      => '18',
            ])],
            $this->userId,
            WorkspaceRole::Viewer,
        );

        self::assertSame('not_your_row', $result['results'][0]['code']);
    }

    public function testUnknownTableIsRejectedWithoutAffectingTheRestOfTheBatch(): void
    {
        $songId = Uuid::generate();

        $result = $this->sync->push(
            $this->db,
            [
                $this->op('users', Uuid::generate(), ['email' => 'x@y.z']),
                $this->op('songs', $songId, ['title' => 'Still applies']),
            ],
            $this->userId,
            WorkspaceRole::Editor,
        );

        self::assertSame('unknown_table', $result['results'][0]['code']);
        self::assertSame('applied', $result['results'][1]['status']);
        self::assertSame('Still applies', $this->db->fetchOne('SELECT title FROM songs WHERE id = ?', [$songId]));
    }

    public function testServerOwnedColumnsCannotBeSetByAPush(): void
    {
        $songId = Uuid::generate();

        $this->sync->push(
            $this->db,
            [$this->op('songs', $songId, [
                'title'      => 'Legitimate',
                'change_seq' => 999999,
                'updated_by' => 'somebody-else',
                'deleted_at' => '2020-01-01T00:00:00.000Z',
            ])],
            $this->userId,
            WorkspaceRole::Editor,
        );

        $row = $this->db->fetchAssociative('SELECT * FROM songs WHERE id = ?', [$songId]);

        self::assertSame(1, (int) $row['change_seq']);
        self::assertSame($this->userId, $row['updated_by']);
        self::assertNull($row['deleted_at']);
    }
}
