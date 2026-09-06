<?php

declare(strict_types=1);

namespace AppTest\Auth;

use App\Account\SessionRepository;
use App\Support\Clock;
use App\Support\Uuid;
use AppTest\ControlTestCase;

/**
 * What rotation does to the windows that are already open.
 *
 * Every refresh replaces the session, and the app is routinely open three times at once —
 * library, control surface, stage. Until this, opening a stage window replaced the session and
 * the control surface's access token stopped working on the spot: mid-service, the one moment
 * it must not. A superseded session now carries the token it issued to its own expiry; a
 * revoked one — a sign-out, or a chain seen twice — stops immediately.
 */
final class SessionRotationTest extends ControlTestCase
{
    private SessionRepository $sessions;
    private string $userId;

    protected function setUp(): void
    {
        parent::setUp();

        $this->sessions = new SessionRepository($this->control, new Clock());
        $this->userId = $this->makeUser('kate@example.com', 'Kate');
    }

    public function testAWindowKeepsWorkingAfterAnotherWindowRefreshes(): void
    {
        $first = $this->open();
        $second = $this->open($first['family_id']);

        $this->sessions->markReplaced($first['id'], $second['id']);

        self::assertTrue(
            $this->sessions->carriesAccessTokens($this->sessions->findById($first['id'])),
            'The window that was open first must not be knocked offline by the one that opened second.',
        );
    }

    /** The replaced session may not be refreshed again: that is what makes reuse detectable. */
    public function testAReplacedSessionCannotBeRefreshed(): void
    {
        $first = $this->open();
        $second = $this->open($first['family_id']);

        $this->sessions->markReplaced($first['id'], $second['id']);

        self::assertFalse($this->sessions->isUsable($this->sessions->findById($first['id'])));
        self::assertTrue($this->sessions->isUsable($this->sessions->findById($second['id'])));
    }

    public function testASignOutStopsTheAccessTokenAtOnce(): void
    {
        $session = $this->open();

        $this->sessions->revoke($session['id']);

        self::assertFalse($this->sessions->carriesAccessTokens($this->sessions->findById($session['id'])));
    }

    /** A stolen chain: every link stops carrying tokens, replaced or not. */
    public function testReuseDetectionKillsTheWholeChainImmediately(): void
    {
        $first = $this->open();
        $second = $this->open($first['family_id']);
        $this->sessions->markReplaced($first['id'], $second['id']);

        $this->sessions->revokeFamily($first['family_id']);

        foreach ([$first['id'], $second['id']] as $id) {
            self::assertFalse(
                $this->sessions->carriesAccessTokens($this->sessions->findById($id)),
                'A chain that has been stolen must stop everywhere, at once.',
            );
        }
    }

    public function testAnExpiredSessionCarriesNothing(): void
    {
        $session = $this->open('', '2020-01-01T00:00:00.000Z');

        self::assertFalse($this->sessions->carriesAccessTokens($this->sessions->findById($session['id'])));
    }

    /** @return array{id: string, family_id: string} */
    private function open(string $familyId = '', string $expiresAt = '2099-01-01T00:00:00.000Z'): array
    {
        return $this->sessions->open(
            $this->userId,
            hash('sha256', Uuid::generate()),
            $expiresAt,
            'PHPUnit',
            $familyId === '' ? null : $familyId,
        );
    }
}
