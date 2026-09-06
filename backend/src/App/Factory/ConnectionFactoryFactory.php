<?php

declare(strict_types=1);

namespace App\Factory;

use App\Database\ConnectionFactory;
use Psr\Container\ContainerInterface;

final class ConnectionFactoryFactory
{
    public function __invoke(ContainerInterface $container): ConnectionFactory
    {
        $config = $container->get('config')['database'] ?? [];

        return new ConnectionFactory((int) ($config['busy_timeout_ms'] ?? 5000));
    }
}
