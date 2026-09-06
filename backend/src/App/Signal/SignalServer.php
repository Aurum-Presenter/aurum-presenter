<?php

declare(strict_types=1);

namespace App\Signal;

use App\Auth\TokenService;
use App\Workspace\WorkspaceRepository;

/**
 * The signalling relay: a post box for two devices trying to find each other on a LAN.
 *
 * It holds no session state, stores no message and never sees a slide, a lyric or a chord. A
 * socket lives until the data channel opens or two minutes pass, whichever comes first.
 *
 * It is a separate process from the API on purpose. PHP-FPM cannot hold a socket open, and the
 * API must not be blocked by one; this loop does nothing but read, check and forward.
 */
final class SignalServer
{
    private const int READ_CHUNK = 8192;
    /** Generous for a human plugging in a tablet, mean for anything automated. */
    private const int MAX_ATTEMPTS_PER_MINUTE = 20;

    /** @var array<int, resource> */
    private array $sockets = [];
    /** @var array<int, string> */
    private array $buffers = [];
    /** @var array<int, bool> */
    private array $upgraded = [];
    /** @var array<string, list<int>> */
    private array $attempts = [];

    private readonly Rooms $rooms;

    public function __construct(
        private readonly TokenService $tokens,
        private readonly WorkspaceRepository $workspaces,
    ) {
        $this->rooms = new Rooms();
    }

    /** @param callable(string): void $log */
    public function run(string $bind, int $port, callable $log, ?int $stopAfterSeconds = null): void
    {
        $server = stream_socket_server(sprintf('tcp://%s:%d', $bind, $port), $code, $message);

        if ($server === false) {
            throw new \RuntimeException(sprintf('Cannot listen on %s:%d — %s', $bind, $port, $message));
        }

        stream_set_blocking($server, false);
        $log(sprintf('Signalling relay listening on %s:%d', $bind, $port));

        $started = time();

        while (true) {
            $read = [$server, ...array_values($this->sockets)];
            $write = null;
            $except = null;

            if (stream_select($read, $write, $except, 1) === false) {
                continue;
            }

            foreach ($read as $stream) {
                if ($stream === $server) {
                    $this->accept($server);
                    continue;
                }

                $this->readFrom($stream, $log);
            }

            foreach ($this->rooms->expired(time()) as $id) {
                $this->closeConnection($id, 4408, 'Pairing timed out.');
            }

            if ($stopAfterSeconds !== null && time() - $started >= $stopAfterSeconds) {
                break;
            }
        }

        fclose($server);
    }

    /** @param resource $server */
    private function accept($server): void
    {
        $socket = @stream_socket_accept($server, 0);

        if ($socket === false) {
            return;
        }

        stream_set_blocking($socket, false);
        $id = (int) $socket;
        $this->sockets[$id] = $socket;
        $this->buffers[$id] = '';
        $this->upgraded[$id] = false;
    }

    /** @param resource $stream */
    private function readFrom($stream, callable $log): void
    {
        $id = (int) $stream;
        $chunk = @fread($stream, self::READ_CHUNK);

        if ($chunk === false || $chunk === '') {
            if (feof($stream)) {
                $this->drop($id);
            }

            return;
        }

        $this->buffers[$id] = ($this->buffers[$id] ?? '') . $chunk;

        if ($this->upgraded[$id] === false) {
            $this->tryUpgrade($id, $log);

            return;
        }

        $this->readFrames($id);
    }

    private function tryUpgrade(int $id, callable $log): void
    {
        try {
            $handshake = Handshake::parse($this->buffers[$id]);
        } catch (ProtocolException $e) {
            $this->write($id, Handshake::reject(400, 'Bad Request'));
            $this->drop($id);
            $log('Refused a connection: ' . $e->getMessage());

            return;
        }

        if ($handshake === null) {
            return;
        }

        $this->buffers[$id] = '';
        $code = $handshake->code();

        if ($code === null) {
            $this->write($id, Handshake::reject(404, 'Not Found'));
            $this->drop($id);

            return;
        }

        $claims = $this->tokens->verifyAccessToken($handshake->query['token'] ?? '');

        if ($claims === null) {
            $this->write($id, Handshake::reject(401, 'Unauthorized'));
            $this->drop($id);

            return;
        }

        $userId = (string) ($claims['sub'] ?? '');

        if (! $this->allowAttempt($userId)) {
            $this->write($id, Handshake::reject(429, 'Too Many Requests'));
            $this->drop($id);

            return;
        }

        // The host opens the room and declares which workspace the session belongs to; the guest
        // is then checked against that same membership. Nothing about the session is stored.
        $host = ! $this->rooms->hasHost($code);
        $workspaceId = $host ? ($handshake->query['workspace'] ?? '') : (string) $this->rooms->workspaceOf($code);

        if ($workspaceId === '' || $this->workspaces->roleOf($userId, $workspaceId) === null) {
            $this->write($id, Handshake::reject(403, 'Forbidden'));
            $this->drop($id);

            return;
        }

        $this->write($id, $handshake->response());
        $this->upgraded[$id] = true;

        if ($host) {
            $this->rooms->open($code, $workspaceId, $id, time());
            $log(sprintf('Room %s opened', $code));

            return;
        }

        if (! $this->rooms->join($code, $id)) {
            // Two peers per code. A third is refused rather than queued, so a stray tab cannot
            // take the place of the tablet someone is holding.
            $this->closeConnection($id, 4409, 'That session already has a device joining.');

            return;
        }

        $log(sprintf('Room %s joined', $code));
    }

    private function readFrames(int $id): void
    {
        while (true) {
            try {
                $frame = Frame::decode($this->buffers[$id]);
            } catch (ProtocolException) {
                $this->closeConnection($id, 1002, 'Protocol error.');

                return;
            }

            if ($frame === null) {
                return;
            }

            if ($frame->opcode === Frame::CLOSE) {
                $this->closeConnection($id, 1000, '');

                return;
            }

            if ($frame->opcode === Frame::PING) {
                $this->write($id, Frame::encode($frame->payload, Frame::PONG));
                continue;
            }

            if ($frame->opcode !== Frame::TEXT) {
                continue;
            }

            if (! Rooms::isSignal($frame->payload)) {
                // Not an offer, an answer or a candidate. The socket is closed rather than
                // asked again: whatever is on the other end is not the app.
                $this->closeConnection($id, 4400, 'Only signalling messages are relayed.');

                return;
            }

            $peer = $this->rooms->peerOf($id);

            if ($peer !== null) {
                $this->write($peer, Frame::encode($frame->payload));
            }
        }
    }

    private function allowAttempt(string $userId): bool
    {
        $now = time();
        $recent = array_values(array_filter(
            $this->attempts[$userId] ?? [],
            static fn (int $at): bool => $now - $at < 60,
        ));

        $recent[] = $now;
        $this->attempts[$userId] = $recent;

        return count($recent) <= self::MAX_ATTEMPTS_PER_MINUTE;
    }

    private function closeConnection(int $id, int $code, string $reason): void
    {
        $this->write($id, Frame::close($code, $reason));

        foreach ($this->rooms->remove($id) as $peer) {
            $this->write($peer, Frame::close(4410, 'The other device left.'));
            $this->drop($peer);
        }

        $this->drop($id);
    }

    private function write(int $id, string $data): void
    {
        $socket = $this->sockets[$id] ?? null;

        if ($socket !== null) {
            @fwrite($socket, $data);
        }
    }

    private function drop(int $id): void
    {
        $socket = $this->sockets[$id] ?? null;

        if ($socket !== null) {
            @fclose($socket);
        }

        unset($this->sockets[$id], $this->buffers[$id], $this->upgraded[$id]);
        $this->rooms->remove($id);
    }
}
