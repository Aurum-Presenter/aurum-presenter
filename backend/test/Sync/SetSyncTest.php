<?php

declare(strict_types=1);

namespace AppTest\Sync;

use App\Database\WriteTransaction;
use App\Enum\WorkspaceRole;
use App\Support\Clock;
use App\Support\Uuid;
use App\Sync\SyncService;
use AppTest\WorkspaceTestCase;

/**
 * Set items after the 0004 rebuild: a fractional rank instead of an integer position, the full
 * non-song vocabulary, and the constraint that an item is either a song or its own content.
 */
final class SetSyncTest extends WorkspaceTestCase
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
    private function op(string $table, string $recordId, array $payload): array
    {
        return [
            'op_id'     => Uuid::generate(),
            'table'     => $table,
            'record_id' => $recordId,
            'op'        => 'upsert',
            'payload'   => $payload,
        ];
    }

    public function testSongItemsAndNonSongItemsBothReplicate(): void
    {
        $setId = Uuid::generate();
        $songId = Uuid::generate();

        $result = $this->sync->push($this->db, [
            $this->op('songs', $songId, ['title' => 'Cornerstone']),
            $this->op('sets', $setId, ['name' => 'Sunday morning', 'venue' => 'Main hall', 'pinned' => 1]),
            $this->op('set_items', Uuid::generate(), [
                'set_id'         => $setId,
                'rank'           => 'a1',
                'song_id'        => $songId,
                'title_snapshot' => 'Cornerstone',
                'key_override'   => 'A',
                'capo_override'  => 2,
                'sections'       => json_encode(['verse-1', 'chorus'], JSON_THROW_ON_ERROR),
            ]),
            $this->op('set_items', Uuid::generate(), [
                'set_id'    => $setId,
                'rank'      => 'a2',
                'item_type' => 'scripture',
                'content'   => 'Psalm 121',
            ]),
        ], $this->userId, WorkspaceRole::Editor);

        self::assertSame(['applied', 'applied', 'applied', 'applied'], array_column($result['results'], 'status'));

        $pull = $this->sync->pull($this->db, 0, $this->userId);
        $items = $pull['tables']['set_items'];
        usort($items, static fn (array $a, array $b): int => strcmp((string) $a['rank'], (string) $b['rank']));

        self::assertSame('Cornerstone', $items[0]['title_snapshot']);
        self::assertSame(2, (int) $items[0]['capo_override']);
        self::assertNull($items[0]['item_type']);
        self::assertSame('scripture', $items[1]['item_type']);
        self::assertSame(1, (int) $pull['tables']['sets'][0]['pinned']);
    }

    /**
     * Business rule 2, enforced by the schema. The push must survive it: one refused row parks
     * that operation and every other operation in the batch still applies.
     */
    public function testAnItemThatIsBothASongAndATextItemIsRejectedWithoutFailingTheBatch(): void
    {
        $setId = Uuid::generate();
        $songId = Uuid::generate();
        $goodItem = Uuid::generate();

        $result = $this->sync->push($this->db, [
            $this->op('songs', $songId, ['title' => 'Cornerstone']),
            $this->op('sets', $setId, ['name' => 'Sunday morning']),
            $this->op('set_items', Uuid::generate(), [
                'set_id' => $setId, 'rank' => 'a1', 'song_id' => $songId, 'item_type' => 'text', 'content' => 'both',
            ]),
            $this->op('set_items', $goodItem, ['set_id' => $setId, 'rank' => 'a2', 'song_id' => $songId]),
        ], $this->userId, WorkspaceRole::Editor);

        $statuses = array_column($result['results'], 'status');

        self::assertSame(['applied', 'applied', 'rejected', 'applied'], $statuses);
        self::assertSame('constraint_violation', $result['results'][2]['code']);

        $items = $this->sync->pull($this->db, 0, $this->userId)['tables']['set_items'];

        self::assertCount(1, $items);
        self::assertSame($goodItem, $items[0]['id']);
    }

    /** Business rule 3: deleting the song must not take the set item with it. */
    public function testDeletingASongLeavesItsSetItemReadable(): void
    {
        $setId = Uuid::generate();
        $songId = Uuid::generate();
        $itemId = Uuid::generate();

        $this->sync->push($this->db, [
            $this->op('songs', $songId, ['title' => 'Cornerstone']),
            $this->op('sets', $setId, ['name' => 'Sunday morning']),
            $this->op('set_items', $itemId, ['set_id' => $setId, 'rank' => 'a1', 'song_id' => $songId, 'title_snapshot' => 'Cornerstone']),
        ], $this->userId, WorkspaceRole::Editor);

        $this->sync->push($this->db, [[
            'op_id' => Uuid::generate(), 'table' => 'songs', 'record_id' => $songId, 'op' => 'delete', 'payload' => [],
        ]], $this->userId, WorkspaceRole::Editor);

        $items = $this->sync->pull($this->db, 0, $this->userId)['tables']['set_items'];

        self::assertCount(1, $items);
        self::assertSame('Cornerstone', $items[0]['title_snapshot']);
    }
}
