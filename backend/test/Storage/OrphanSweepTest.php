<?php

declare(strict_types=1);

namespace AppTest\Storage;

use App\Storage\OrphanSweep;
use PHPUnit\Framework\TestCase;

/**
 * When a stored file is finally deleted.
 *
 * Replacing a sheet writes a new object and leaves the old one alone, so a device that is still
 * holding the old URL keeps receiving the bytes it cached (s3 acceptance criterion 6). The old
 * object goes on the same thirty-day horizon as a tombstone — and never before, because a file
 * the object store has and the database has not heard about yet may be the only copy there is.
 */
final class OrphanSweepTest extends TestCase
{
    private const int CUTOFF = 1_000_000;

    /** @return list<array{key: string, size: int, modified: int}> */
    private function objects(): array
    {
        return [
            ['key' => 'sheets/ws/aaa.pdf', 'size' => 10, 'modified' => self::CUTOFF - 86400],
            ['key' => 'sheets/ws/bbb.pdf', 'size' => 10, 'modified' => self::CUTOFF - 86400],
            ['key' => 'assets/ws/ccc.png', 'size' => 10, 'modified' => self::CUTOFF - 86400],
            ['key' => 'sheets/ws/ddd.pdf', 'size' => 10, 'modified' => self::CUTOFF + 60],
        ];
    }

    public function testAFileStillNamedByARowIsKept(): void
    {
        self::assertNotContains('sheets/ws/aaa.pdf', OrphanSweep::orphans($this->objects(), ['aaa'], self::CUTOFF));
    }

    public function testTheReplacedFileIsSweptOnceItIsOldEnough(): void
    {
        self::assertContains('sheets/ws/bbb.pdf', OrphanSweep::orphans($this->objects(), ['aaa'], self::CUTOFF));
    }

    /** A background nothing points at is the same kind of thing as an unreferenced sheet. */
    public function testBackgroundsAreSweptToo(): void
    {
        self::assertContains('assets/ws/ccc.png', OrphanSweep::orphans($this->objects(), ['aaa'], self::CUTOFF));
        self::assertNotContains('assets/ws/ccc.png', OrphanSweep::orphans($this->objects(), ['ccc'], self::CUTOFF));
    }

    public function testAFileWrittenInsideTheWindowIsLeftAloneWhateverTheDatabaseSays(): void
    {
        self::assertNotContains('sheets/ws/ddd.pdf', OrphanSweep::orphans($this->objects(), [], self::CUTOFF));
    }

    public function testHashesAreComparedWithoutCase(): void
    {
        $objects = [['key' => 'sheets/ws/ABC.pdf', 'size' => 1, 'modified' => 0]];

        self::assertSame([], OrphanSweep::orphans($objects, ['abc'], self::CUTOFF));
    }
}
