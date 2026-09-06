<?php

declare(strict_types=1);

namespace App\Discovery;

use Tempest\Discovery\Discovery;
use Tempest\Discovery\DiscoveryItems;
use Tempest\Discovery\DiscoveryLocation;
use Tempest\Reflection\ClassReflector;
use Tempest\Support\Filesystem;
use Tempest\Support\VarExport\VarExportPhpFile;

final class DiscoveryManager
{
    /** @var Discovery[] */
    private array $discoveries = [];
    /** @var array<string, DiscoveryItems> */
    private array $discoveryItemsMap = [];

    /**
     * @param array<string, mixed> $config
     */
    public function __construct(
        private readonly array $config,
        private readonly string $rootPath,
    ) {
    }

    public function addDiscovery(Discovery $discovery): self
    {
        $className = get_class($discovery);
        $this->discoveries[$className] = $discovery;
        $this->discoveryItemsMap[$className] = new DiscoveryItems();
        return $this;
    }

    public function discover(bool $force = false): void
    {
        $cacheEnabled = $this->config['discovery_cache'] ?? false;
        $cachePath = $this->config['discovery_cache_path'] ?? null;

        if (! $force && $cacheEnabled && $cachePath && Filesystem\is_file($this->rootPath . '/' . $cachePath)) {
            $cacheFile = new VarExportPhpFile($this->rootPath . '/' . $cachePath);
            $cachedItemsMap = $cacheFile->import();

            if (is_array($cachedItemsMap)) {
                foreach ($cachedItemsMap as $discoveryClass => $items) {
                    if ($items instanceof DiscoveryItems && isset($this->discoveries[$discoveryClass])) {
                        $this->discoveryItemsMap[$discoveryClass] = $items;
                        $this->discoveries[$discoveryClass]->setItems($items);
                    }
                }
            }

            return;
        }

        foreach ($this->config['discovery_locations'] ?? [] as $locationConfig) {
            $location = new DiscoveryLocation(
                $locationConfig['namespace'],
                $this->rootPath . '/' . $locationConfig['path']
            );

            $this->discoverLocation($location);
        }

        if ($cacheEnabled && $cachePath) {
            $this->cache($this->rootPath . '/' . $cachePath);
        }
    }

    private function discoverLocation(DiscoveryLocation $location): void
    {
        $files = $this->scanDirectory($location->path);

        foreach ($files as $file) {
            $className = $location->toClassName($file);

            if (! class_exists($className)) {
                continue;
            }

            $classReflector = new ClassReflector($className);

            foreach ($this->discoveries as $discoveryClass => $discovery) {
                $discovery->setItems($this->discoveryItemsMap[$discoveryClass]);
                $discovery->discover($location, $classReflector);
            }
        }
    }

    /**
     * @return string[]
     */
    private function scanDirectory(string $directory): array
    {
        $files = [];

        foreach (Filesystem\list_directory($directory) as $node) {
            if (Filesystem\is_directory($node)) {
                $files = [...$files, ...$this->scanDirectory($node)];
            } elseif (str_ends_with($node, '.php')) {
                $files[] = $node;
            }
        }

        return $files;
    }

    public function cache(string $path): void
    {
        $cacheFile = new VarExportPhpFile($path);
        $cacheFile->export($this->discoveryItemsMap);
    }

    public function clearCache(string $path): void
    {
        if (Filesystem\is_file($path)) {
            unlink($path);
        }
    }

    /**
     * @return array<string, DiscoveryItems>
     */
    public function getDiscoveryItemsMap(): array
    {
        return $this->discoveryItemsMap;
    }
}
