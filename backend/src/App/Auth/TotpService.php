<?php

declare(strict_types=1);

namespace App\Auth;

use OTPHP\TOTP;
use SensitiveParameter;

/**
 * RFC 6238 TOTP: 30-second step, 6 digits, one step of leeway either side for clock drift.
 *
 * Replay protection is the part that is easy to omit and easy to exploit. A code stays valid
 * for its whole 30-second window, so without recording the last accepted step, an attacker who
 * observes a code over someone's shoulder can reuse it. `lastAcceptedStep` is stored per user
 * and every verification must be strictly newer.
 */
final class TotpService
{
    public const int PERIOD = 30;
    public const int DIGITS = 6;
    /**
     * Seconds of clock drift accepted either side, which otphp requires to be *inside* the
     * period — a leeway of a whole period would mean a code was valid in every window, and it
     * refuses that outright. One second short of the period is the widest it allows, and the
     * step-matching loop below still only accepts the three steps around now.
     */
    public const int LEEWAY = self::PERIOD - 1;

    public function __construct(
        private readonly SecretCipher $cipher,
        private readonly string $issuer = 'Aurum Presenter',
    ) {
    }

    public function generateSecret(): string
    {
        return TOTP::generate()->getSecret();
    }

    public function encryptSecret(#[SensitiveParameter] string $secret): string
    {
        return $this->cipher->encrypt($secret);
    }

    public function provisioningUri(#[SensitiveParameter] string $secret, string $accountEmail): string
    {
        $totp = TOTP::createFromSecret($secret);
        $totp->setLabel($accountEmail);
        $totp->setIssuer($this->issuer);

        return $totp->getProvisioningUri();
    }

    /**
     * @param string      $encryptedSecret as stored in totp_secrets.secret_encrypted
     * @param int|null    $lastAcceptedStep as stored in totp_secrets.last_accepted_step
     *
     * @return int|null the accepted step when valid, null when the code is wrong or replayed.
     *                  The caller must persist the returned step before issuing a session.
     */
    public function verify(
        string $encryptedSecret,
        #[SensitiveParameter] string $code,
        ?int $lastAcceptedStep,
        ?int $now = null,
    ): ?int {
        $code = preg_replace('/\s+/', '', $code) ?? '';
        if (! preg_match('/^\d{' . self::DIGITS . '}$/', $code)) {
            return null;
        }

        $secret = $this->cipher->decrypt($encryptedSecret);
        $totp = TOTP::createFromSecret($secret);
        $now ??= time();

        if (! $totp->verify($code, $now, self::LEEWAY)) {
            return null;
        }

        // Identify which step actually matched, so a replay inside the same window is refused.
        for ($offset = -1; $offset <= 1; $offset++) {
            $timestamp = $now + ($offset * self::PERIOD);
            $step = intdiv($timestamp, self::PERIOD);

            if (! hash_equals($totp->at($timestamp), $code)) {
                continue;
            }

            if ($lastAcceptedStep !== null && $step <= $lastAcceptedStep) {
                return null;
            }

            return $step;
        }

        return null;
    }
}
