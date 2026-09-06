<?php

declare(strict_types=1);

namespace App\Discovery;

use App\Attribute\Route;
use Tempest\Discovery\Discovery;
use Tempest\Discovery\DiscoveryLocation;
use Tempest\Discovery\IsDiscovery;
use Tempest\Reflection\ClassReflector;

final class RouteDiscovery implements Discovery
{
    use IsDiscovery;

    /**
     * @param ClassReflector<object> $class
     */
    public function discover(DiscoveryLocation $location, ClassReflector $class): void
    {
        foreach ($class->getPublicMethods() as $method) {
            foreach ($method->getAttributes(Route::class) as $route) {
                $this->discoveryItems->add($location, [
                    'path' => $route->path,
                    'middleware' => $class->getName(),
                    'allowed_methods' => $route->methods,
                    'name' => $route->name,
                    'options' => $route->options,
                ]);
            }
        }
    }

    public function getConfigKey(): string
    {
        return 'routes';
    }

    public function apply(): void
    {
    }
}
