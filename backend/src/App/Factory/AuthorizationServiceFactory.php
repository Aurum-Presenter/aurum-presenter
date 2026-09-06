<?php

declare(strict_types=1);

namespace App\Factory;

use App\Auth\AuthorizationService;
use Psr\Container\ContainerInterface;

final class AuthorizationServiceFactory
{
    public function __invoke(ContainerInterface $container): AuthorizationService
    {
        return new AuthorizationService($container->get('config')['authorization']['roles'] ?? []);
    }
}
