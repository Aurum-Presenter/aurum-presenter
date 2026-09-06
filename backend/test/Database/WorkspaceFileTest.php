<?php

declare(strict_types=1);

namespace AppTest\Database;

use App\Database\ConnectionFactory;
use App\Database\Migrator;
use App\Database\WorkspaceDatabase;
use App\Support\Uuid;
use PHPUnit\Framework\TestCase;

/**
 * The file *is* the workspace, so the properties the change request asks for are properties of
 * files: a denied request opens none, twenty writers can share one, a created one is sound, and
 * a deleted one leaves nothing behind — including the WAL sidecars, which would otherwise hold
 * the last transactions of a workspace somebody asked to be rid of.
 */
final class WorkspaceFileTest extends TestCase
{
    private string $directory;
    private WorkspaceDatabase $databases;

    protected function setUp(): void
    {
        $this->directory = sys_get_temp_dir() . '/aurum-files-' . bin2hex(random_bytes(6));
        mkdir($this->directory, 0o775, true);

        $this->databases = new WorkspaceDatabase(
            new ConnectionFactory(),
            new Migrator(dirname(__DIR__, 2) . '/migrations/workspace'),
            $this->directory,
            true,
        );
    }

    protected function tearDown(): void
    {
        foreach (glob($this->directory . '/*') ?: [] as $file) {
            unlink($file);
        }

        @rmdir($this->directory);
    }

    public function testACreatedWorkspaceFileIsSound(): void
    {
        $id = Uuid::generate();
        $db = $this->databases->open($id);

        self::assertSame('ok', $db->fetchOne('PRAGMA integrity_check'));
        self::assertSame('wal', strtolower((string) $db->fetchOne('PRAGMA journal_mode')));
    }

    public function testDeletingAWorkspaceLeavesNoFileBehind(): void
    {
        $id = Uuid::generate();
        $db = $this->databases->open($id);
        $db->insert('folders', [
            'id' => Uuid::generate(), 'name' => 'Hymns', 'position' => 0,
            'updated_at' => '2026-09-06T00:00:00.000Z', 'change_seq' => 1,
        ]);

        self::assertTrue($this->databases->exists($id));

        $this->databases->delete($id);

        self::assertFalse($this->databases->exists($id));
        self::assertSame([], glob($this->directory . '/' . $id . '*') ?: []);
    }

    /**
     * Twenty writers, in twenty processes, against one file. Without WAL and a busy timeout this
     * is where SQLITE_BUSY shows up — and a musician's edit would be the thing that failed.
     */
    public function testTwentyConcurrentWritersAllCommit(): void
    {
        $id = Uuid::generate();
        $this->databases->open($id);
        $path = $this->databases->pathFor($id);

        $script = dirname(__DIR__) . '/fixtures/concurrent-writer.php';
        $processes = [];

        for ($writer = 0; $writer < 20; $writer++) {
            $processes[] = proc_open(
                sprintf('%s %s %s %d', escapeshellarg(PHP_BINARY), escapeshellarg($script), escapeshellarg($path), $writer),
                [1 => ['pipe', 'w'], 2 => ['pipe', 'w']],
                $pipes[$writer],
            );
        }

        $failures = [];

        foreach ($processes as $writer => $process) {
            if ($process === false) {
                self::markTestSkipped('This environment will not start subprocesses.');
            }

            $output = stream_get_contents($pipes[$writer][1]) . stream_get_contents($pipes[$writer][2]);
            fclose($pipes[$writer][1]);
            fclose($pipes[$writer][2]);

            if (proc_close($process) !== 0) {
                $failures[] = sprintf('writer %d: %s', $writer, trim($output));
            }
        }

        self::assertSame([], $failures, 'Every writer must commit; SQLITE_BUSY is not an acceptable answer.');

        $db = (new ConnectionFactory())->open($path);

        self::assertSame(20, (int) $db->fetchOne('SELECT COUNT(*) FROM folders'));
        self::assertSame('ok', $db->fetchOne('PRAGMA integrity_check'));
    }
}
