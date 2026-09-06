<?php

declare(strict_types=1);

namespace App\Factory;

use App\Auth\TokenService;
use App\Support\Clock;
use Psr\Container\ContainerInterface;

final class TokenServiceFactory
{
    public function __invoke(ContainerInterface $container): TokenService
    {
        $config = $container->get('config')['auth'] ?? [];

        return new TokenService(
            $container->get(Clock::class),
            $config['signing_key'],
            (int) ($config['access_ttl'] ?? 900),
            (int) ($config['refresh_ttl'] ?? 2592000),
        );
    }
}
