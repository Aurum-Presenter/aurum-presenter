<?php

declare(strict_types=1);

namespace AppTest\Auth;

use App\Auth\SecretCipher;
use App\Auth\TotpService;
use OTPHP\TOTP;
use PHPUnit\Framework\TestCase;

/**
 * The second factor, end to end: a secret is generated, encrypted, and a code from an
 * authenticator holding that secret verifies against it.
 *
 * This exists because the leeway was set to a whole period, which the library refuses outright
 * — so every enrolment failed with a 500, and an account could never gain the second factor
 * that owning a band workspace requires.
 */
final class TotpServiceTest extends TestCase
{
    private TotpService $totp;

    protected function setUp(): void
    {
        $this->totp = new TotpService(new SecretCipher(base64_encode(str_repeat('k', 32))));
    }

    public function testACodeFromAnAuthenticatorVerifies(): void
    {
        $secret = $this->totp->generateSecret();
        $encrypted = $this->totp->encryptSecret($secret);
        $now = time();

        $code = TOTP::createFromSecret($secret)->at($now);

        self::assertSame(intdiv($now, TotpService::PERIOD), $this->totp->verify($encrypted, $code, null, $now));
    }

    /**
     * Clock drift of a step either side is accepted; three steps out is not.
     *
     * The instant is fixed at the middle of a window rather than taken from the clock. The
     * leeway is a second short of a full period — otphp will not accept a leeway equal to the
     * period — so a code exactly one step away is forgiven from anywhere but the last second of
     * a window, and a test that used the real clock would fail on those seconds alone.
     */
    public function testOneStepOfDriftIsForgiven(): void
    {
        $secret = $this->totp->generateSecret();
        $encrypted = $this->totp->encryptSecret($secret);
        $now = intdiv(time(), TotpService::PERIOD) * TotpService::PERIOD + intdiv(TotpService::PERIOD, 2);
        $authenticator = TOTP::createFromSecret($secret);

        self::assertNotNull($this->totp->verify($encrypted, $authenticator->at($now - TotpService::PERIOD), null, $now));
        self::assertNotNull($this->totp->verify($encrypted, $authenticator->at($now + TotpService::PERIOD), null, $now));
        self::assertNull($this->totp->verify($encrypted, $authenticator->at($now + 3 * TotpService::PERIOD), null, $now));
    }

    /**
     * A code stays valid for its whole window, so without this an attacker who reads one over a
     * shoulder can use it again inside the same thirty seconds.
     */
    public function testACodeCannotBeUsedTwice(): void
    {
        $secret = $this->totp->generateSecret();
        $encrypted = $this->totp->encryptSecret($secret);
        $now = time();
        $code = TOTP::createFromSecret($secret)->at($now);

        $step = $this->totp->verify($encrypted, $code, null, $now);

        self::assertNotNull($step);
        self::assertNull($this->totp->verify($encrypted, $code, $step, $now), 'A replayed code must be refused.');
    }

    public function testNonsenseIsRefusedWithoutTouchingTheSecret(): void
    {
        $encrypted = $this->totp->encryptSecret($this->totp->generateSecret());

        self::assertNull($this->totp->verify($encrypted, '12345', null));
        self::assertNull($this->totp->verify($encrypted, 'abcdef', null));
    }

    public function testTheProvisioningUriCarriesTheIssuerAndAccount(): void
    {
        $uri = $this->totp->provisioningUri($this->totp->generateSecret(), 'kate@example.com');

        self::assertStringStartsWith('otpauth://totp/', $uri);
        self::assertStringContainsString('kate%40example.com', $uri);
        self::assertStringContainsString('issuer=Aurum%20Presenter', $uri);
    }
}
