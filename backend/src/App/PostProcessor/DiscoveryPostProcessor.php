<?php

declare(strict_types=1);

namespace App\PostProcessor;

use App\Discovery\DiscoveryManager;
use App\Discovery\RouteDiscovery;

class DiscoveryPostProcessor
{
    /**
     * @param array<string, mixed> $config
     * @return array<string, mixed>
     */
    public function __invoke(array $config): array
    {
        $discoveryConfig = $config['discovery'] ?? [];
        $rootPath = defined('APP_DIR') ? APP_DIR : realpath(__DIR__ . '/../../../');

        $discoveryManager = new DiscoveryManager($discoveryConfig, $rootPath);
        $routeDiscovery = new RouteDiscovery();
        $discoveryManager->addDiscovery($routeDiscovery);

        $discoveryManager->discover();

        $discoveryItemsMap = $discoveryManager->getDiscoveryItemsMap();

        foreach ($discoveryItemsMap as $discoveryClass => $items) {
            /** @var \Tempest\Discovery\Discovery $discovery */
            $discovery = new $discoveryClass();
            if (method_exists($discovery, 'getConfigKey')) {
                $configKey = $discovery->getConfigKey();
                $config[$configKey] = array_merge($config[$configKey] ?? [], iterator_to_array($items));
            }
        }

        return $config;
    }
}
