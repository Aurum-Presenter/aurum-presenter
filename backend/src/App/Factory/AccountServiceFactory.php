<?php

declare(strict_types=1);

namespace App\Factory;

use App\Account\AccountRepository;
use App\Auth\AccountService;
use App\Auth\PasswordHasher;
use App\Database\WorkspaceDatabase;
use App\Workspace\WorkspaceRepository;
use Psr\Container\ContainerInterface;

final class AccountServiceFactory
{
    public function __invoke(ContainerInterface $container): AccountService
    {
        $config = $container->get('config')['auth'] ?? [];

        return new AccountService(
            $container->get(AccountRepository::class),
            $container->get(WorkspaceRepository::class),
            $container->get(WorkspaceDatabase::class),
            $container->get(PasswordHasher::class),
            (int) ($config['min_password_length'] ?? 12),
            (int) ($config['lockout_threshold'] ?? 5),
            (int) ($config['lockout_window'] ?? 900),
        );
    }
}
