<?php

declare(strict_types=1);

namespace App\Middleware;

use Laminas\Diactoros\Response\EmptyResponse;
use Psr\Http\Message\ResponseInterface;
use Psr\Http\Message\ServerRequestInterface;
use Psr\Http\Server\MiddlewareInterface;
use Psr\Http\Server\RequestHandlerInterface;

/**
 * The API and the PWA are separate origins in development (Vite on :5173, API on :8080), so
 * CORS is not optional even locally. Credentials are allowed because the refresh token travels
 * as an httpOnly cookie, which in turn forbids a wildcard origin — the origin is echoed only
 * when it is on the configured allow list.
 */
final class CorsMiddleware implements MiddlewareInterface
{
    /** @param string[] $allowedOrigins */
    public function __construct(private readonly array $allowedOrigins)
    {
    }

    public function process(ServerRequestInterface $request, RequestHandlerInterface $handler): ResponseInterface
    {
        $origin = $request->getHeaderLine('Origin');
        $allowed = $origin !== '' && in_array($origin, $this->allowedOrigins, true);

        if (strtoupper($request->getMethod()) === 'OPTIONS') {
            return $this->decorate(new EmptyResponse(204), $origin, $allowed);
        }

        return $this->decorate($handler->handle($request), $origin, $allowed);
    }

    private function decorate(ResponseInterface $response, string $origin, bool $allowed): ResponseInterface
    {
        if (! $allowed) {
            return $response;
        }

        return $response
            ->withHeader('Access-Control-Allow-Origin', $origin)
            ->withHeader('Access-Control-Allow-Credentials', 'true')
            ->withHeader('Access-Control-Allow-Methods', 'GET, POST, PATCH, PUT, DELETE, OPTIONS')
            ->withHeader('Access-Control-Allow-Headers', 'Authorization, Content-Type, If-Match')
            ->withHeader('Access-Control-Max-Age', '600')
            ->withHeader('Vary', 'Origin');
    }
}
