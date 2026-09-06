<?php

declare(strict_types=1);

if (PHP_SAPI === 'cli-server' && $_SERVER['SCRIPT_FILENAME'] !== __FILE__) {
    return false;
}

chdir(dirname(__DIR__));
require 'vendor/autoload.php';

(function (): void {
    /** @var \Psr\Container\ContainerInterface $container */
    $container = require 'config/container.php';

    $container->get(\Mezzio\Application::class)->run();
})();
