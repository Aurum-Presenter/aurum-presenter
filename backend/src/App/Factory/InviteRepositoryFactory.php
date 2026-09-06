<?php

declare(strict_types=1);

namespace App\Factory;

use App\Database\ControlDatabase;
use App\Invite\InviteRepository;
use App\Support\Clock;
use App\Support\Env;
use Psr\Container\ContainerInterface;

final class InviteRepositoryFactory
{
    public function __invoke(ContainerInterface $container): InviteRepository
    {
        $key = Env::string('APP_SIGNING_KEY');

        if ($key === null || $key === '') {
            throw new \RuntimeException('APP_SIGNING_KEY is required to hash invitation tokens.');
        }

        return new InviteRepository(
            $container->get(ControlDatabase::class),
            $container->get(Clock::class),
            $key,
        );
    }
}
