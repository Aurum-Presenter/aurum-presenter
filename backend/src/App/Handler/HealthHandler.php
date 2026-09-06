<?php

declare(strict_types=1);

namespace App\Handler;

use App\Attribute\Route;
use App\Database\ControlDatabase;
use App\Http\Json;
use App\Http\RouteOptions;
use Psr\Http\Message\ResponseInterface;
use Psr\Http\Message\ServerRequestInterface;
use Psr\Http\Server\RequestHandlerInterface;
use Throwable;

final class HealthHandler implements RequestHandlerInterface
{
    public function __construct(private readonly ControlDatabase $control)
    {
    }

    #[Route(
        path: '/api/v1/health',
        methods: ['GET'],
        name: 'health',
        options: [RouteOptions::PUBLIC => true],
    )]
    public function handle(ServerRequestInterface $request): ResponseInterface
    {
        try {
            $this->control->connection()->fetchOne('SELECT 1');
            $database = 'ok';
        } catch (Throwable $e) {
            $database = 'error: ' . $e->getMessage();
        }

        return Json::ok([
            'status'   => $database === 'ok' ? 'ok' : 'degraded',
            'database' => $database,
            'time'     => gmdate('c'),
        ], $database === 'ok' ? 200 : 503);
    }
}
