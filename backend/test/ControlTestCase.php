<?php

declare(strict_types=1);

namespace AppTest;

use App\Database\ConnectionFactory;
use App\Database\ControlDatabase;
use App\Database\Migrator;
use PHPUnit\Framework\TestCase;

/**
 * Base class for tests that need the identity tier: accounts, workspaces, memberships, invites.
 * Each test gets its own control.sqlite, so nothing here has to undo what it did.
 */
abstract class ControlTestCase extends TestCase
{
    protected string $directory;
    protected ControlDatabase $control;

    protected function setUp(): void
    {
        $this->directory = sys_get_temp_dir() . '/aurum-control-' . bin2hex(random_bytes(6));
        mkdir($this->directory, 0o775, true);

        $this->control = new ControlDatabase(
            new ConnectionFactory(),
            new Migrator(dirname(__DIR__) . '/migrations/control'),
            $this->directory . '/control.sqlite',
            true,
        );

        $this->control->connection();
    }

    protected function tearDown(): void
    {
        foreach (glob($this->directory . '/*') ?: [] as $file) {
            unlink($file);
        }

        @rmdir($this->directory);
    }

    /** @return string the new user's id */
    protected function makeUser(string $email, string $displayName = 'Someone'): string
    {
        $id = \App\Support\Uuid::generate();

        $this->control->connection()->insert('users', [
            'id'           => $id,
            'email'        => $email,
            'display_name' => $displayName,
            'created_at'   => gmdate('Y-m-d\TH:i:s.v\Z'),
            'updated_at'   => gmdate('Y-m-d\TH:i:s.v\Z'),
        ]);

        return $id;
    }
}
