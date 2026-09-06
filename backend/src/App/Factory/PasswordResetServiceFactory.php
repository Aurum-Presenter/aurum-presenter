<?php

declare(strict_types=1);

namespace App\Factory;

use App\Account\AccountRepository;
use App\Account\SessionRepository;
use App\Auth\PasswordHasher;
use App\Auth\PasswordResetService;
use App\Database\ControlDatabase;
use App\Support\Clock;
use App\Support\Env;
use Psr\Container\ContainerInterface;

final class PasswordResetServiceFactory
{
    public function __invoke(ContainerInterface $container): PasswordResetService
    {
        $key = Env::string('APP_SIGNING_KEY');

        if ($key === null || $key === '') {
            throw new \RuntimeException('APP_SIGNING_KEY is required to hash password reset tokens.');
        }

        return new PasswordResetService(
            $container->get(ControlDatabase::class),
            $container->get(AccountRepository::class),
            $container->get(SessionRepository::class),
            $container->get(PasswordHasher::class),
            $container->get(Clock::class),
            $key,
        );
    }
}
