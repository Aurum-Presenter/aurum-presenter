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
 * Preferred key, capo and reading layout are personal. Two members of the same band read the
 * same chart in two different keys, and neither is aware the other did anything — which is
 * acceptance criterion 5 of the chord-chart feature, and the reason the pull is scoped by user.
 */
final class PreferenceSyncTest extends WorkspaceTestCase
{
    private SyncService $sync;

    protected function setUp(): void
    {
        parent::setUp();

        $this->sync = new SyncService(new WriteTransaction(), new Clock());
    }

    /** @param array<string, mixed> $value */
    private function preference(string $userId, string $songId, array $value): array
    {
        return [
            'op_id'     => Uuid::generate(),
            'table'     => 'preferences',
            'record_id' => Uuid::generate(),
            'op'        => 'upsert',
            'payload'   => [
                'user_id'    => $userId,
                'scope_type' => 'song',
                'scope_id'   => $songId,
                'name'       => 'chart',
                'value'      => json_encode($value, JSON_THROW_ON_ERROR),
            ],
        ];
    }

    public function testEachMemberPullsOnlyTheirOwnPreferences(): void
    {
        $songId = Uuid::generate();
        $alice = Uuid::generate();
        $bob = Uuid::generate();

        $this->sync->push($this->db, [$this->preference($alice, $songId, ['preferred_key' => 'G'])], $alice, WorkspaceRole::Editor);
        $this->sync->push($this->db, [$this->preference($bob, $songId, ['preferred_key' => 'Bb'])], $bob, WorkspaceRole::Viewer);

        $hers = $this->sync->pull($this->db, 0, $alice)['tables']['preferences'];
        $his = $this->sync->pull($this->db, 0, $bob)['tables']['preferences'];

        self::assertCount(1, $hers);
        self::assertCount(1, $his);
        self::assertStringContainsString('"G"', (string) $hers[0]['value']);
        self::assertStringContainsString('"Bb"', (string) $his[0]['value']);
    }

    /** Role has nothing to do with it: nobody writes anybody else's preferences. */
    public function testWritingAnotherMembersPreferenceIsRejected(): void
    {
        $songId = Uuid::generate();
        $owner = Uuid::generate();
        $someoneElse = Uuid::generate();

        $result = $this->sync->push(
            $this->db,
            [$this->preference($someoneElse, $songId, ['preferred_key' => 'A'])],
            $owner,
            WorkspaceRole::Owner,
        );

        self::assertSame('rejected', $result['results'][0]['status']);
        self::assertSame('not_your_row', $result['results'][0]['code']);
        self::assertSame([], $this->sync->pull($this->db, 0, $someoneElse)['tables']['preferences']);
    }
}
