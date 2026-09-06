<?php

declare(strict_types=1);

namespace App\Factory;

use App\Auth\SecretCipher;
use Psr\Container\ContainerInterface;

final class SecretCipherFactory
{
    public function __invoke(ContainerInterface $container): SecretCipher
    {
        return new SecretCipher($container->get('config')['auth']['secret_key']);
    }
}
