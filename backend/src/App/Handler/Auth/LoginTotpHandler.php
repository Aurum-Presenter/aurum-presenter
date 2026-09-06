<?php

declare(strict_types=1);

namespace App\Handler\Auth;

use App\Account\AccountRepository;
use App\Attribute\Route;
use App\Auth\RecoveryCodeService;
use App\Auth\SessionIssuer;
use App\Auth\TotpService;
use App\Http\ApiException;
use App\Http\Json;
use App\Http\RouteOptions;
use App\Support\Clock;
use Psr\Http\Message\ResponseInterface;
use Psr\Http\Message\ServerRequestInterface;
use Psr\Http\Server\RequestHandlerInterface;

/**
 * Exchanges a pending-2FA challenge for a session, using either a TOTP code or a recovery code.
 */
final class LoginTotpHandler implements RequestHandlerInterface
{
    private const int MAX_ATTEMPTS = 5;

    public function __construct(
        private readonly AccountRepository $accounts,
        private readonly TotpService $totp,
        private readonly RecoveryCodeService $recovery,
        private readonly SessionIssuer $issuer,
        private readonly Clock $clock,
    ) {
    }

    #[Route(
        path: '/api/v1/auth/login/totp',
        methods: ['POST'],
        name: 'auth.login.totp',
        options: [RouteOptions::PUBLIC => true],
    )]
    public function handle(ServerRequestInterface $request): ResponseInterface
    {
        $body = Json::body($request);
        $challengeId = Json::requireString($body, 'challenge_id', 64);
        $code = Json::requireString($body, 'code', 32);

        $challenge = $this->accounts->findChallenge($challengeId);

        if ($challenge === null || $this->clock->isPast((string) $challenge['expires_at'])) {
            throw ApiException::unauthorized('This sign-in attempt has expired. Start again.', 'challenge_expired');
        }

        if ((int) $challenge['attempts'] >= self::MAX_ATTEMPTS) {
            $this->accounts->deleteChallenge($challengeId);

            throw ApiException::unauthorized('Too many incorrect codes. Start again.', 'challenge_exhausted');
        }

        $userId = (string) $challenge['user_id'];
        $secret = $this->accounts->confirmedTotp($userId);

        if ($secret === null) {
            throw ApiException::unauthorized('No second factor is enrolled.', 'totp_not_enrolled');
        }

        $step = $this->totp->verify(
            (string) $secret['secret_encrypted'],
            $code,
            $secret['last_accepted_step'] === null ? null : (int) $secret['last_accepted_step'],
        );

        if ($step !== null) {
            $this->accounts->recordTotpStep($userId, $step);
        } elseif (! $this->accounts->consumeRecoveryCode($userId, $this->recovery->hash($code))) {
            $this->accounts->countChallengeAttempt($challengeId);

            throw ApiException::unauthorized('That code is not valid.', 'invalid_totp');
        }

        $this->accounts->deleteChallenge($challengeId);
        $issued = $this->issuer->issue($userId, $request);
        $issued['body']['recovery_codes_remaining'] = $this->accounts->remainingRecoveryCodes($userId);

        return $this->issuer->attach(Json::ok($issued['body']), $issued['cookie']);
    }
}
