<?php

declare(strict_types=1);

namespace App\Signal;

/**
 * Who is paired with whom, for as long as it takes to pair them.
 *
 * This is the whole of the relay's state, it lives in memory, and it is gone when the process
 * restarts — which is exactly right for a post box. A room holds the workspace its host
 * declared, so the guest can be checked against the same membership rule as every other route,
 * and nothing else about the session ever reaches the server.
 */
final class Rooms
{
    /** A pairing that has not completed in this long is abandoned (CR: two minutes). */
    public const int TTL_SECONDS = 120;

    /** @var array<string, array{workspace: string, host: int, guest: int|null, opened_at: int}> */
    private array $rooms = [];

    public function open(string $code, string $workspaceId, int $host, int $now): void
    {
        $this->rooms[$code] = ['workspace' => $workspaceId, 'host' => $host, 'guest' => null, 'opened_at' => $now];
    }

    public function workspaceOf(string $code): ?string
    {
        return $this->rooms[$code]['workspace'] ?? null;
    }

    public function hasHost(string $code): bool
    {
        return isset($this->rooms[$code]);
    }

    /** At most two peers per code; a third is refused rather than queued. */
    public function join(string $code, int $guest): bool
    {
        if (! isset($this->rooms[$code]) || $this->rooms[$code]['guest'] !== null) {
            return false;
        }

        $this->rooms[$code]['guest'] = $guest;

        return true;
    }

    /** The other end of the room, which is the only place a message may go. */
    public function peerOf(int $connection): ?int
    {
        foreach ($this->rooms as $room) {
            if ($room['host'] === $connection) {
                return $room['guest'];
            }

            if ($room['guest'] === $connection) {
                return $room['host'];
            }
        }

        return null;
    }

    /** @return list<int> the connections left over when a room is torn down */
    public function remove(int $connection): array
    {
        foreach ($this->rooms as $code => $room) {
            if ($room['host'] !== $connection && $room['guest'] !== $connection) {
                continue;
            }

            unset($this->rooms[$code]);

            return array_values(array_filter(
                [$room['host'], $room['guest']],
                static fn (?int $peer): bool => $peer !== null && $peer !== $connection,
            ));
        }

        return [];
    }

    /** @return list<int> connections in rooms that have taken too long */
    public function expired(int $now): array
    {
        $stale = [];

        foreach ($this->rooms as $code => $room) {
            if ($now - $room['opened_at'] < self::TTL_SECONDS) {
                continue;
            }

            $stale[] = $room['host'];

            if ($room['guest'] !== null) {
                $stale[] = $room['guest'];
            }

            unset($this->rooms[$code]);
        }

        return $stale;
    }

    public function count(): int
    {
        return count($this->rooms);
    }

    /**
     * The only messages the relay will carry: an offer, an answer, an ICE candidate. Anything
     * else — a slide, a lyric, a chord — is not signalling and is refused.
     */
    public static function isSignal(string $payload): bool
    {
        if (strlen($payload) > Frame::MAX_PAYLOAD) {
            return false;
        }

        try {
            $decoded = json_decode($payload, true, 8, JSON_THROW_ON_ERROR);
        } catch (\JsonException) {
            return false;
        }

        if (! is_array($decoded)) {
            return false;
        }

        return match ($decoded['kind'] ?? null) {
            'offer', 'answer' => is_string($decoded['sdp'] ?? null),
            'ice' => is_array($decoded['candidate'] ?? null),
            default => false,
        };
    }
}
