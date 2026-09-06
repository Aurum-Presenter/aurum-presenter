<?php

declare(strict_types=1);

namespace App\Factory;

use App\Middleware\ApiErrorHandlerMiddleware;
use Psr\Container\ContainerInterface;
use Psr\Log\LoggerInterface;

final class ApiErrorHandlerMiddlewareFactory
{
    public function __invoke(ContainerInterface $container): ApiErrorHandlerMiddleware
    {
        return new ApiErrorHandlerMiddleware(
            (bool) ($container->get('config')['debug'] ?? false),
            $container->has(LoggerInterface::class) ? $container->get(LoggerInterface::class) : null,
        );
    }
}
