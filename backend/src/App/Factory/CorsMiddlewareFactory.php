<?php

declare(strict_types=1);

namespace App\Factory;

use App\Middleware\CorsMiddleware;
use Psr\Container\ContainerInterface;

final class CorsMiddlewareFactory
{
    public function __invoke(ContainerInterface $container): CorsMiddleware
    {
        return new CorsMiddleware($container->get('config')['cors']['allowed_origins'] ?? []);
    }
}
