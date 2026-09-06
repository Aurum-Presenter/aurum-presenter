<?php

declare(strict_types=1);

namespace App;

use Laminas\ServiceManager\Factory\InvokableFactory;
use Mezzio\Application;
use Mezzio\Container\ApplicationConfigInjectionDelegator;

/**
 * Only services whose constructors need scalars from config are listed here. Everything else —
 * repositories, middleware, handlers — is built by laminas-di's autowiring abstract factory,
 * which resolves object-typed constructor parameters on its own.
 */
class ConfigProvider
{
    /** @return array<string, mixed> */
    public function __invoke(): array
    {
        return [
            'dependencies' => $this->getDependencies(),
            'laminas-cli'  => [
                'commands' => [
                    'migrate'           => Command\MigrateCommand::class,
                    'workspace:list'    => Command\ListWorkspacesCommand::class,
                    'maintenance:purge' => Command\PurgeCommand::class,
                    'signal:serve'      => Command\SignalCommand::class,
                ],
            ],
        ];
    }

    /** @return array<string, mixed> */
    public function getDependencies(): array
    {
        return [
            'delegators' => [
                // Reads config['routes'] (populated by DiscoveryPostProcessor from #[Route]
                // attributes) and config['middleware_pipeline'] into the Application.
                Application::class => [ApplicationConfigInjectionDelegator::class],
            ],
            'aliases' => [
                Storage\ObjectStore::class => 'ObjectStore',
            ],
            'factories' => [
                Support\Clock::class              => InvokableFactory::class,
                Database\WriteTransaction::class  => InvokableFactory::class,

                Database\ConnectionFactory::class => Factory\ConnectionFactoryFactory::class,
                Database\ControlDatabase::class   => Factory\ControlDatabaseFactory::class,
                Database\WorkspaceDatabase::class => Factory\WorkspaceDatabaseFactory::class,

                Auth\AuthorizationService::class  => Factory\AuthorizationServiceFactory::class,
                Auth\PasswordHasher::class        => Factory\PasswordHasherFactory::class,
                Auth\SecretCipher::class          => Factory\SecretCipherFactory::class,
                Auth\TokenService::class          => Factory\TokenServiceFactory::class,
                Auth\TotpService::class           => Factory\TotpServiceFactory::class,
                Auth\RecoveryCodeService::class   => Factory\RecoveryCodeServiceFactory::class,
                Auth\SessionIssuer::class         => Factory\SessionIssuerFactory::class,
                Auth\AccountService::class        => Factory\AccountServiceFactory::class,

                'ObjectStore'                     => Factory\ObjectStoreFactory::class,
                Signal\SignalServer::class        => Factory\SignalServerFactory::class,

                Middleware\ApiErrorHandlerMiddleware::class => Factory\ApiErrorHandlerMiddlewareFactory::class,
                Middleware\CorsMiddleware::class            => Factory\CorsMiddlewareFactory::class,
            ],
        ];
    }
}
