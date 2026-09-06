<?php

declare(strict_types=1);

namespace App\Http;

use Laminas\Diactoros\Response\JsonResponse;
use Psr\Http\Message\ResponseInterface;
use Psr\Http\Message\ServerRequestInterface;

final class Json
{
    /** @param array<string, mixed>|list<mixed> $data */
    public static function ok(array $data = [], int $status = 200): ResponseInterface
    {
        return new JsonResponse($data, $status);
    }

    public static function noContent(): ResponseInterface
    {
        return new JsonResponse(null, 204);
    }

    /**
     * Body parsing is done by BodyParamsMiddleware; this only narrows the result to an array so
     * handlers can stop re-checking.
     *
     * @return array<string, mixed>
     */
    public static function body(ServerRequestInterface $request): array
    {
        $body = $request->getParsedBody();

        return is_array($body) ? $body : [];
    }

    /** @param array<string, mixed> $body */
    public static function requireString(array $body, string $key, int $maxLength = 1000): string
    {
        $value = $body[$key] ?? null;

        if (! is_string($value) || trim($value) === '') {
            throw ApiException::unprocessable(
                sprintf('"%s" is required.', $key),
                ['field' => $key],
                'validation_failed'
            );
        }

        if (mb_strlen($value) > $maxLength) {
            throw ApiException::unprocessable(
                sprintf('"%s" must be at most %d characters.', $key, $maxLength),
                ['field' => $key],
                'validation_failed'
            );
        }

        return trim($value);
    }

    /** @param array<string, mixed> $body */
    public static function optionalString(array $body, string $key): ?string
    {
        $value = $body[$key] ?? null;

        return is_string($value) && trim($value) !== '' ? trim($value) : null;
    }

    /**
     * @param array<string, mixed> $body
     * @return array<array-key, mixed>
     */
    public static function requireList(array $body, string $key): array
    {
        $value = $body[$key] ?? null;

        if (! is_array($value)) {
            throw ApiException::unprocessable(
                sprintf('"%s" must be an array.', $key),
                ['field' => $key],
                'validation_failed'
            );
        }

        return $value;
    }
}
