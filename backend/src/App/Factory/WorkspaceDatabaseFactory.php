<?php

declare(strict_types=1);

namespace App\Factory;

use App\Database\ConnectionFactory;
use App\Database\Migrator;
use App\Database\WorkspaceDatabase;
use Psr\Container\ContainerInterface;

final class WorkspaceDatabaseFactory
{
    public function __invoke(ContainerInterface $container): WorkspaceDatabase
    {
        $config = $container->get('config')['database'] ?? [];

        return new WorkspaceDatabase(
            $container->get(ConnectionFactory::class),
            new Migrator($config['workspace_migrations']),
            $config['workspace_dir'],
            (bool) ($config['auto_migrate'] ?? true),
        );
    }
}
