<?php

declare(strict_types=1);

namespace App\Factory;

use App\Account\SessionRepository;
use App\Auth\SessionIssuer;
use App\Auth\TokenService;
use Psr\Container\ContainerInterface;

final class SessionIssuerFactory
{
    public function __invoke(ContainerInterface $container): SessionIssuer
    {
        $config = $container->get('config')['auth'] ?? [];

        return new SessionIssuer(
            $container->get(TokenService::class),
            $container->get(SessionRepository::class),
            $config['cookie_path'] ?? '/api/v1/auth',
            (bool) ($config['cookie_secure'] ?? true),
        );
    }
}
