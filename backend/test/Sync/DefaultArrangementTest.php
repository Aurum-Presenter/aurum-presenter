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
 * Exactly one arrangement of a song is the default.
 *
 * Two people offline can each pick a different one, and both are real edits — so the later
 * write demotes the others instead of being refused. A push that came back rejected would be a
 * musician's change lost to a race, which is the one thing sync must not do. The file carries a
 * partial unique index to say the invariant holds; this is what keeps it true.
 */
final class DefaultArrangementTest extends WorkspaceTestCase
{
    private SyncService $sync;
    private string $songId;

    protected function setUp(): void
    {
        parent::setUp();

        $this->sync = new SyncService(new WriteTransaction(), new Clock());
        $this->songId = Uuid::generate();

        $this->push('songs', $this->songId, ['title' => 'Amazing Grace']);
    }

    public function testMakingOneTheDefaultDemotesTheOther(): void
    {
        $first = $this->arrangement('Original', true);
        $second = $this->arrangement('Acoustic', false);

        $this->push('arrangements', $second, ['is_default' => 1]);

        self::assertSame(0, $this->defaultFlagOf($first));
        self::assertSame(1, $this->defaultFlagOf($second));
    }

    public function testTheDemotionTravelsWithTheSameChangeSequence(): void
    {
        $first = $this->arrangement('Original', true);
        $second = $this->arrangement('Acoustic', false);

        $this->push('arrangements', $second, ['is_default' => 1]);

        self::assertSame(
            (int) $this->db->fetchOne('SELECT change_seq FROM arrangements WHERE id = ?', [$second]),
            (int) $this->db->fetchOne('SELECT change_seq FROM arrangements WHERE id = ?', [$first]),
            'A device pulling the promotion must pull the demotion in the same delta.',
        );
    }

    /** The second device's push is applied, not rejected: nothing a musician did is lost. */
    public function testTwoDevicesChoosingDifferentDefaultsBothSucceed(): void
    {
        $first = $this->arrangement('Original', true);
        $second = $this->arrangement('Acoustic', false);
        $third = $this->arrangement('Half time', false);

        $results = [
            $this->push('arrangements', $second, ['is_default' => 1]),
            $this->push('arrangements', $third, ['is_default' => 1]),
        ];

        foreach ($results as $result) {
            self::assertSame('applied', $result['results'][0]['status']);
        }

        self::assertSame(0, $this->defaultFlagOf($first));
        self::assertSame(0, $this->defaultFlagOf($second));
        self::assertSame(1, $this->defaultFlagOf($third));
        self::assertSame(1, (int) $this->db->fetchOne(
            'SELECT COUNT(*) FROM arrangements WHERE song_id = ? AND is_default = 1 AND deleted_at IS NULL',
            [$this->songId],
        ));
    }

    /** The same holds for an arrangement created as the default, not only one promoted later. */
    public function testANewArrangementCreatedAsTheDefaultDemotesTheOldOne(): void
    {
        $first = $this->arrangement('Original', true);
        $second = $this->arrangement('Acoustic', true);

        self::assertSame(0, $this->defaultFlagOf($first));
        self::assertSame(1, $this->defaultFlagOf($second));
    }

    /** A deleted arrangement that was the default must not block the one that replaces it. */
    public function testATombstonedDefaultDoesNotBlockANewOne(): void
    {
        $first = $this->arrangement('Original', true);

        $this->sync->push($this->db, [[
            'op_id' => Uuid::generate(), 'table' => 'arrangements', 'record_id' => $first,
            'op' => 'delete', 'payload' => [],
        ]], 'user-1', WorkspaceRole::Editor);

        $second = $this->arrangement('Acoustic', true);

        self::assertSame(1, $this->defaultFlagOf($second));
    }

    private function arrangement(string $name, bool $isDefault): string
    {
        $id = Uuid::generate();

        $this->push('arrangements', $id, [
            'song_id'    => $this->songId,
            'name'       => $name,
            'body'       => '{title: x}',
            'is_default' => $isDefault ? 1 : 0,
        ]);

        return $id;
    }

    /** @param array<string, mixed> $payload @return array<string, mixed> */
    private function push(string $table, string $recordId, array $payload): array
    {
        return $this->sync->push($this->db, [[
            'op_id'     => Uuid::generate(),
            'table'     => $table,
            'record_id' => $recordId,
            'op'        => 'upsert',
            'payload'   => $payload,
        ]], 'user-1', WorkspaceRole::Editor);
    }

    private function defaultFlagOf(string $id): int
    {
        return (int) $this->db->fetchOne('SELECT is_default FROM arrangements WHERE id = ?', [$id]);
    }
}
