<?php

declare(strict_types=1);

namespace App\Factory;

use App\Database\ConnectionFactory;
use App\Database\ControlDatabase;
use App\Database\Migrator;
use Psr\Container\ContainerInterface;

final class ControlDatabaseFactory
{
    public function __invoke(ContainerInterface $container): ControlDatabase
    {
        $config = $container->get('config')['database'] ?? [];

        return new ControlDatabase(
            $container->get(ConnectionFactory::class),
            new Migrator($config['control_migrations']),
            $config['control_path'],
            (bool) ($config['auto_migrate'] ?? true),
        );
    }
}
