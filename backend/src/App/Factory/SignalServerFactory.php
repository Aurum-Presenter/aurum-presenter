<?php

declare(strict_types=1);

namespace App\Factory;

use App\Auth\TokenService;
use App\Signal\SignalServer;
use App\Workspace\WorkspaceRepository;
use Psr\Container\ContainerInterface;

final class SignalServerFactory
{
    public function __invoke(ContainerInterface $container): SignalServer
    {
        return new SignalServer(
            $container->get(TokenService::class),
            $container->get(WorkspaceRepository::class),
        );
    }
}
