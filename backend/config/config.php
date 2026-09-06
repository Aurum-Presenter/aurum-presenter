<?php

declare(strict_types=1);

use Laminas\ConfigAggregator\ArrayProvider;
use Laminas\ConfigAggregator\ConfigAggregator;
use Laminas\ConfigAggregator\PhpFileProvider;

// APP_ENV is the single switch for environment-dependent behaviour. There is no
// laminas-development-mode here: no development.config.php, no `composer development-enable`.
$appEnv        = getenv('APP_ENV') ?: 'production';
$isDevelopment = $appEnv === 'development';
$isTest        = $appEnv === 'test';

// A null path — not merely ENABLE_CACHE => false — is what actually disables the cache.
// ConfigAggregator reads an existing cache file unconditionally and only consults
// ENABLE_CACHE when deciding whether to write one, so a cache left behind by a production
// build could otherwise leak into a development request.
$configCachePath = $isDevelopment || $isTest ? null : 'data/cache/config-cache.php';

// DiscoveryManager resolves discovery_locations relative to this constant.
if (! defined('APP_DIR')) {
    define('APP_DIR', realpath(__DIR__ . '/../'));
}

$aggregator = new ConfigAggregator([
    \Laminas\Di\ConfigProvider::class,
    \Mezzio\Helper\ConfigProvider::class,
    \Mezzio\Router\LaminasRouter\ConfigProvider::class,
    \Laminas\Router\ConfigProvider::class,
    \Laminas\HttpHandlerRunner\ConfigProvider::class,
    \Mezzio\ConfigProvider::class,
    \Mezzio\Router\ConfigProvider::class,
    \Laminas\Diactoros\ConfigProvider::class,

    App\ConfigProvider::class,

    // Application config, loaded so local overrides global (first to last):
    //   global.php, *.global.php, local.php, *.local.php
    new PhpFileProvider(realpath(__DIR__) . '/autoload/{{,*.}global,{,*.}local}.php'),

    // Environment-specific overrides, loaded last so they win.
    new PhpFileProvider(sprintf('%s/autoload/{{,*.}%s}.php', realpath(__DIR__), $appEnv)),

    // Aggregated last so nothing can override the cache/debug decision.
    new ArrayProvider([
        'debug'                        => $isDevelopment,
        'app_env'                      => $appEnv,
        'config_cache_path'            => $configCachePath,
        ConfigAggregator::ENABLE_CACHE => ! $isDevelopment && ! $isTest,
    ]),
], $configCachePath, [
    // Scans src/App for #[Route] attributes and merges them into config['routes'].
    // MUST be a post-processor: it runs after all providers merge, and its output is
    // baked into the config cache alongside everything else.
    new App\PostProcessor\DiscoveryPostProcessor(),
]);

return $aggregator->getMergedConfig();
