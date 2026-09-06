<?php

declare(strict_types=1);

namespace App\Http;

use RuntimeException;
use Throwable;

/**
 * A failure that is a normal, expected outcome of a request rather than a defect. These carry a
 * stable machine-readable `code` alongside the status, so the client can branch on the reason
 * without parsing prose.
 */
class ApiException extends RuntimeException
{
    /** @param array<string, mixed> $details */
    public function __construct(
        private readonly int $status,
        private readonly string $errorCode,
        string $message,
        private readonly array $details = [],
        ?Throwable $previous = null,
    ) {
        parent::__construct($message, $status, $previous);
    }

    public function status(): int
    {
        return $this->status;
    }

    public function errorCode(): string
    {
        return $this->errorCode;
    }

    /** @return array<string, mixed> */
    public function details(): array
    {
        return $this->details;
    }

    /** @param array<string, mixed> $details */
    public static function badRequest(string $message, array $details = [], string $code = 'bad_request'): self
    {
        return new self(400, $code, $message, $details);
    }

    public static function unauthorized(string $message = 'Authentication required.', string $code = 'unauthorized'): self
    {
        return new self(401, $code, $message);
    }

    public static function forbidden(string $message = 'You do not have access to this.', string $code = 'forbidden'): self
    {
        return new self(403, $code, $message);
    }

    public static function notFound(string $message = 'Not found.', string $code = 'not_found'): self
    {
        return new self(404, $code, $message);
    }

    public static function conflict(string $message, string $code = 'conflict'): self
    {
        return new self(409, $code, $message);
    }

    /** @param array<string, mixed> $details */
    public static function unprocessable(string $message, array $details = [], string $code = 'unprocessable'): self
    {
        return new self(422, $code, $message, $details);
    }

    public static function tooManyRequests(string $message, int $retryAfter = 0): self
    {
        return new self(429, 'too_many_requests', $message, ['retry_after' => $retryAfter]);
    }
}
