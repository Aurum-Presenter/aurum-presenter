<?php

declare(strict_types=1);

namespace App\Factory;

use App\Auth\RecoveryCodeService;
use Psr\Container\ContainerInterface;

final class RecoveryCodeServiceFactory
{
    public function __invoke(ContainerInterface $container): RecoveryCodeService
    {
        return new RecoveryCodeService($container->get('config')['auth']['signing_key']);
    }
}
