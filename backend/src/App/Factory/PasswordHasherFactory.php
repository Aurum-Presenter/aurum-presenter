<?php

declare(strict_types=1);

namespace App\Factory;

use App\Auth\PasswordHasher;
use Psr\Container\ContainerInterface;

final class PasswordHasherFactory
{
    public function __invoke(ContainerInterface $container): PasswordHasher
    {
        return new PasswordHasher($container->get('config')['auth']['argon2'] ?? []);
    }
}
