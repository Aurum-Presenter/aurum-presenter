<?php

declare(strict_types=1);

namespace AppTest;

use App\Database\ConnectionFactory;
use App\Database\Migrator;
use Doctrine\DBAL\Connection;
use PHPUnit\Framework\TestCase;

/**
 * Base class for tests that need a real workspace database. Each test gets its own file, which
 * is the same isolation the production design gives each workspace — so nothing here has to
 * clean up shared state between tests.
 */
abstract class WorkspaceTestCase extends TestCase
{
    protected string $path;
    protected Connection $db;

    protected function setUp(): void
    {
        $directory = sys_get_temp_dir() . '/aurum-test-' . bin2hex(random_bytes(6));
        mkdir($directory, 0o775, true);

        $this->path = $directory . '/workspace.sqlite';
        $this->db = (new ConnectionFactory())->open($this->path);

        (new Migrator(dirname(__DIR__) . '/migrations/workspace'))->migrate($this->db);
    }

    protected function tearDown(): void
    {
        $this->db->close();

        foreach (glob($this->path . '*') ?: [] as $file) {
            unlink($file);
        }

        @rmdir(dirname($this->path));
    }
}
