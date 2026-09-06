<?php

declare(strict_types=1);

namespace App\Middleware;

use App\Http\ApiException;
use Laminas\Diactoros\Response\JsonResponse;
use Psr\Http\Message\ResponseInterface;
use Psr\Http\Message\ServerRequestInterface;
use Psr\Http\Server\MiddlewareInterface;
use Psr\Http\Server\RequestHandlerInterface;
use Psr\Log\LoggerInterface;
use Throwable;

/**
 * Outermost middleware: turns every uncaught throwable into a JSON error envelope.
 *
 * An API that answers an unexpected failure with an HTML error page breaks the sync engine's
 * retry logic, which classifies responses by status and expects a parseable body.
 */
final class ApiErrorHandlerMiddleware implements MiddlewareInterface
{
    public function __construct(
        private readonly bool $debug,
        private readonly ?LoggerInterface $logger = null,
    ) {
    }

    public function process(ServerRequestInterface $request, RequestHandlerInterface $handler): ResponseInterface
    {
        try {
            return $handler->handle($request);
        } catch (ApiException $e) {
            return new JsonResponse([
                'error' => array_filter([
                    'code'    => $e->errorCode(),
                    'message' => $e->getMessage(),
                    'details' => $e->details() ?: null,
                ], static fn ($v) => $v !== null),
            ], $e->status());
        } catch (Throwable $e) {
            $this->logger?->error($e->getMessage(), ['exception' => $e]);

            $payload = [
                'error' => [
                    'code'    => 'internal_error',
                    'message' => 'Something went wrong.',
                ],
            ];

            if ($this->debug) {
                $payload['error']['debug'] = [
                    'exception' => $e::class,
                    'message'   => $e->getMessage(),
                    'file'      => $e->getFile() . ':' . $e->getLine(),
                    'trace'     => explode("\n", $e->getTraceAsString()),
                ];
            }

            return new JsonResponse($payload, 500);
        }
    }
}
