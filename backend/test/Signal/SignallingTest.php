<?php

declare(strict_types=1);

namespace AppTest\Signal;

use App\Signal\Frame;
use App\Signal\Handshake;
use App\Signal\ProtocolException;
use App\Signal\Rooms;
use PHPUnit\Framework\TestCase;

/**
 * The relay's protocol layer. It carries an offer, an answer and ICE candidates between two
 * devices on a LAN, and refuses everything else — including anything that looks like session
 * content, which must never reach the server.
 */
final class SignallingTest extends TestCase
{
    /** Client frames are masked; the relay has to unmask them and must refuse ones that are not. */
    public function testDecodesAMaskedClientFrame(): void
    {
        $payload = '{"kind":"offer","sdp":"v=0"}';
        $buffer = self::clientFrame($payload);

        $frame = Frame::decode($buffer);

        self::assertNotNull($frame);
        self::assertSame(Frame::TEXT, $frame->opcode);
        self::assertSame($payload, $frame->payload);
        self::assertSame('', $buffer, 'The frame should be consumed from the buffer.');
    }

    public function testWaitsForTheRestOfAFrame(): void
    {
        $whole = self::clientFrame(str_repeat('a', 400));
        $partial = substr($whole, 0, 20);

        self::assertNull(Frame::decode($partial));
        self::assertSame(20, strlen($partial), 'An incomplete frame must stay in the buffer.');
    }

    public function testRefusesAnUnmaskedFrame(): void
    {
        $buffer = Frame::encode('unmasked');

        $this->expectException(ProtocolException::class);
        Frame::decode($buffer);
    }

    public function testRefusesAFrameOverTheSizeLimit(): void
    {
        $buffer = self::clientFrame(str_repeat('x', Frame::MAX_PAYLOAD + 1));

        $this->expectException(ProtocolException::class);
        Frame::decode($buffer);
    }

    public function testRefusesAFragmentedFrame(): void
    {
        $buffer = self::clientFrame('part', final: false);

        $this->expectException(ProtocolException::class);
        Frame::decode($buffer);
    }

    public function testEncodesTheThreeLengthForms(): void
    {
        self::assertSame(2 + 5, strlen(Frame::encode('short')));
        self::assertSame(4 + 200, strlen(Frame::encode(str_repeat('a', 200))));
        self::assertSame(10 + 70000, strlen(Frame::encode(str_repeat('a', 70000))));
    }

    public function testHandshakeAcceptsTheKeyFromTheSpecification(): void
    {
        $request = "GET /api/v1/sessions/4KJ9QP/signal?token=abc&workspace=w1 HTTP/1.1\r\n"
            . "Host: aurum.local\r\n"
            . "Upgrade: websocket\r\n"
            . "Connection: Upgrade\r\n"
            . "Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n"
            . "Sec-WebSocket-Version: 13\r\n\r\n";

        $handshake = Handshake::parse($request);

        self::assertNotNull($handshake);
        self::assertSame('4KJ9QP', $handshake->code());
        self::assertSame('abc', $handshake->query['token']);
        self::assertSame('w1', $handshake->query['workspace']);

        // The example key and accept value from RFC 6455 §1.3.
        self::assertStringContainsString('Sec-WebSocket-Accept: s3pPLMBiTxaQ9kYGzzhZRbK+xOo=', $handshake->response());
    }

    public function testHandshakeRefusesWhatIsNotAnUpgrade(): void
    {
        $this->expectException(ProtocolException::class);
        Handshake::parse("GET /api/v1/health HTTP/1.1\r\nHost: x\r\n\r\n");
    }

    public function testHandshakeWaitsForTheWholeRequest(): void
    {
        self::assertNull(Handshake::parse("GET / HTTP/1.1\r\nUpgrade: websocket\r\n"));
    }

    public function testOnlyTheSignallingPathHasACode(): void
    {
        $request = "GET /api/v1/sessions/ABC/signal HTTP/1.1\r\nUpgrade: websocket\r\nSec-WebSocket-Key: k\r\n\r\n";

        self::assertNull(Handshake::parse($request)?->code(), 'A five-character code is not a code.');
    }

    /** At most two peers per code; a third is refused rather than queued. */
    public function testARoomHoldsExactlyTwoPeers(): void
    {
        $rooms = new Rooms();
        $rooms->open('4KJ9QP', 'workspace-1', 10, 1000);

        self::assertSame('workspace-1', $rooms->workspaceOf('4KJ9QP'));
        self::assertTrue($rooms->join('4KJ9QP', 11));
        self::assertFalse($rooms->join('4KJ9QP', 12), 'A third device must be refused.');
        self::assertSame(11, $rooms->peerOf(10));
        self::assertSame(10, $rooms->peerOf(11));
        self::assertNull($rooms->peerOf(12));
    }

    public function testLeavingTearsDownTheRoomAndNamesTheOtherPeer(): void
    {
        $rooms = new Rooms();
        $rooms->open('4KJ9QP', 'workspace-1', 10, 1000);
        $rooms->join('4KJ9QP', 11);

        self::assertSame([11], $rooms->remove(10));
        self::assertFalse($rooms->hasHost('4KJ9QP'));
        self::assertSame(0, $rooms->count());
    }

    public function testAPairingThatNeverCompletesExpires(): void
    {
        $rooms = new Rooms();
        $rooms->open('4KJ9QP', 'workspace-1', 10, 1000);
        $rooms->join('4KJ9QP', 11);

        self::assertSame([], $rooms->expired(1000 + Rooms::TTL_SECONDS - 1));
        self::assertSame([10, 11], $rooms->expired(1000 + Rooms::TTL_SECONDS));
        self::assertSame(0, $rooms->count());
    }

    /**
     * The rule that keeps the relay a post box: it carries setup traffic, and a message that is
     * not setup traffic closes the socket rather than being forwarded.
     */
    public function testOnlySignallingMessagesAreCarried(): void
    {
        self::assertTrue(Rooms::isSignal('{"kind":"offer","sdp":"v=0\r\n"}'));
        self::assertTrue(Rooms::isSignal('{"kind":"answer","sdp":"v=0"}'));
        self::assertTrue(Rooms::isSignal('{"kind":"ice","candidate":{"candidate":"candidate:1 1 UDP"}}'));

        self::assertFalse(Rooms::isSignal('{"type":"state","state":{"slides":[]}}'));
        self::assertFalse(Rooms::isSignal('{"kind":"offer"}'));
        self::assertFalse(Rooms::isSignal('not json'));
        self::assertFalse(Rooms::isSignal('"just a string"'));
        self::assertFalse(Rooms::isSignal('{"kind":"offer","sdp":"' . str_repeat('a', Frame::MAX_PAYLOAD) . '"}'));
    }

    private static function clientFrame(string $payload, bool $final = true): string
    {
        $mask = 'abcd';
        $length = strlen($payload);
        $header = chr(($final ? 0x80 : 0x00) | Frame::TEXT);

        if ($length < 126) {
            $header .= chr(0x80 | $length);
        } elseif ($length < 65536) {
            $header .= chr(0x80 | 126) . pack('n', $length);
        } else {
            $header .= chr(0x80 | 127) . pack('J', $length);
        }

        $masked = '';
        for ($index = 0; $index < $length; $index++) {
            $masked .= $payload[$index] ^ $mask[$index % 4];
        }

        return $header . $mask . $masked;
    }
}
