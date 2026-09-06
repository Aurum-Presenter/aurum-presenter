<?php

declare(strict_types=1);

namespace App\Factory;

use App\Auth\SecretCipher;
use App\Auth\TotpService;
use Psr\Container\ContainerInterface;

final class TotpServiceFactory
{
    public function __invoke(ContainerInterface $container): TotpService
    {
        return new TotpService(
            $container->get(SecretCipher::class),
            $container->get('config')['auth']['totp_issuer'] ?? 'Aurum Presenter',
        );
    }
}
