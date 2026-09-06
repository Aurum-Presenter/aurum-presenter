<?php

declare(strict_types=1);

namespace App\Signal;

/**
 * The HTTP request that turns into a WebSocket.
 *
 * The token arrives in the query string rather than a header because a browser will not let a
 * page set headers on a WebSocket handshake. It is the fifteen-minute access token, on a socket
 * that lives two minutes, over the same TLS as the rest of the API.
 */
final class Handshake
{
    private const string GUID = '258EAFA5-E914-47DA-95CA-C5AB0DC85B11';

    /**
     * @param array<string, string> $query
     */
    private function __construct(
        public readonly string $path,
        public readonly array $query,
        public readonly string $key,
    ) {
    }

    /** Returns null while the request is incomplete, and throws on one that is not a handshake. */
    public static function parse(string $request): ?self
    {
        if (! str_contains($request, "\r\n\r\n")) {
            return null;
        }

        $lines = explode("\r\n", $request);
        $start = explode(' ', $lines[0] ?? '');

        if (($start[0] ?? '') !== 'GET') {
            throw new ProtocolException('Only GET can be upgraded.');
        }

        $headers = [];

        foreach (array_slice($lines, 1) as $line) {
            if ($line === '') {
                break;
            }

            [$name, $value] = array_pad(explode(':', $line, 2), 2, '');
            $headers[strtolower(trim($name))] = trim($value);
        }

        if (strtolower($headers['upgrade'] ?? '') !== 'websocket') {
            throw new ProtocolException('Not a WebSocket upgrade.');
        }

        $key = $headers['sec-websocket-key'] ?? '';

        if ($key === '') {
            throw new ProtocolException('Missing Sec-WebSocket-Key.');
        }

        $target = $start[1] ?? '/';
        $path = parse_url($target, PHP_URL_PATH) ?: '/';
        parse_str((string) parse_url($target, PHP_URL_QUERY), $query);

        /** @var array<string, string> $query */
        return new self($path, array_map(strval(...), $query), $key);
    }

    public function response(): string
    {
        $accept = base64_encode(sha1($this->key . self::GUID, true));

        return "HTTP/1.1 101 Switching Protocols\r\n"
            . "Upgrade: websocket\r\n"
            . "Connection: Upgrade\r\n"
            . "Sec-WebSocket-Accept: {$accept}\r\n\r\n";
    }

    public static function reject(int $status, string $reason): string
    {
        return sprintf("HTTP/1.1 %d %s\r\nConnection: close\r\nContent-Length: 0\r\n\r\n", $status, $reason);
    }

    /** The pairing code out of `/api/v1/sessions/{code}/signal`, or null if that is not the path. */
    public function code(): ?string
    {
        // Codes are typed by a person on a dark stage, so they arrive in whatever case the
        // keyboard was in; the alphabet itself has no ambiguous characters.
        if (preg_match('#^/api/v1/sessions/([A-Za-z2-9]{6})/signal$#', $this->path, $matches) !== 1) {
            return null;
        }

        return strtoupper($matches[1]);
    }
}
