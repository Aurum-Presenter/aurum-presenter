<?php

declare(strict_types=1);

namespace App\Signal;

/**
 * The RFC 6455 framing the relay needs, and nothing else.
 *
 * A dependency was considered and rejected: the relay speaks text frames, ping, pong and close,
 * on a socket that lives for at most two minutes. That is a hundred lines of well-specified
 * bit-twiddling, against a library that would have to be kept current for the lifetime of the
 * project. Everything unusual — continuation frames, extensions, compression — is refused
 * rather than half-implemented.
 */
final class Frame
{
    public const int TEXT = 0x1;
    public const int BINARY = 0x2;
    public const int CLOSE = 0x8;
    public const int PING = 0x9;
    public const int PONG = 0xA;

    /** The signalling envelope is SDP or ICE; 16 KB is generous for both (CR: relay rules). */
    public const int MAX_PAYLOAD = 16384;

    public function __construct(
        public readonly int $opcode,
        public readonly string $payload,
    ) {
    }

    /**
     * Reads one frame off the front of the buffer, consuming it. Returns null when the buffer
     * does not yet hold a whole frame — the caller keeps reading and tries again.
     *
     * @throws ProtocolException on anything this relay refuses to speak
     */
    public static function decode(string &$buffer): ?self
    {
        if (strlen($buffer) < 2) {
            return null;
        }

        $first = ord($buffer[0]);
        $second = ord($buffer[1]);

        $final = ($first & 0x80) !== 0;
        $opcode = $first & 0x0F;
        $masked = ($second & 0x80) !== 0;
        $length = $second & 0x7F;
        $offset = 2;

        if (! $final) {
            throw new ProtocolException('Fragmented frames are not accepted.');
        }

        // A client frame must be masked (RFC 6455 §5.1). An unmasked one is either a broken
        // client or something pretending to be one.
        if (! $masked) {
            throw new ProtocolException('Client frames must be masked.');
        }

        if ($length === 126) {
            if (strlen($buffer) < $offset + 2) {
                return null;
            }

            $length = unpack('n', substr($buffer, $offset, 2))[1];
            $offset += 2;
        } elseif ($length === 127) {
            if (strlen($buffer) < $offset + 8) {
                return null;
            }

            $high = unpack('N', substr($buffer, $offset, 4))[1];
            $low = unpack('N', substr($buffer, $offset + 4, 4))[1];
            $length = ($high << 32) | $low;
            $offset += 8;
        }

        if ($length > self::MAX_PAYLOAD) {
            throw new ProtocolException(sprintf('Frame of %d bytes exceeds the %d byte limit.', $length, self::MAX_PAYLOAD));
        }

        if (strlen($buffer) < $offset + 4 + $length) {
            return null;
        }

        $mask = substr($buffer, $offset, 4);
        $offset += 4;
        $masked_payload = substr($buffer, $offset, $length);
        $offset += $length;

        $payload = '';
        for ($index = 0; $index < $length; $index++) {
            $payload .= $masked_payload[$index] ^ $mask[$index % 4];
        }

        $buffer = substr($buffer, $offset);

        return new self($opcode, $payload);
    }

    /** Server frames are never masked, which is the other half of RFC 6455 §5.1. */
    public static function encode(string $payload, int $opcode = self::TEXT): string
    {
        $length = strlen($payload);
        $header = chr(0x80 | $opcode);

        if ($length < 126) {
            $header .= chr($length);
        } elseif ($length < 65536) {
            $header .= chr(126) . pack('n', $length);
        } else {
            $header .= chr(127) . pack('J', $length);
        }

        return $header . $payload;
    }

    /** A close frame carrying a status code the client can act on. */
    public static function close(int $code, string $reason = ''): string
    {
        return self::encode(pack('n', $code) . $reason, self::CLOSE);
    }
}
