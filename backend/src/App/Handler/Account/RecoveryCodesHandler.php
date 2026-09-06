<?php

declare(strict_types=1);

namespace App\Handler\Account;

use App\Account\AccountRepository;
use App\Attribute\Route;
use App\Auth\PasswordHasher;
use App\Auth\RecoveryCodeService;
use App\Auth\TotpService;
use App\Http\ApiException;
use App\Http\Json;
use App\Middleware\SessionMiddleware;
use Psr\Http\Message\ResponseInterface;
use Psr\Http\Message\ServerRequestInterface;
use Psr\Http\Server\RequestHandlerInterface;

/**
 * New recovery codes.
 *
 * Password *and* a current code: regenerating is how somebody with a borrowed session would
 * give themselves a permanent way back in, so it is gated on both factors, and the old codes
 * stop working the moment the new ones are shown.
 */
final class RecoveryCodesHandler implements RequestHandlerInterface
{
    public function __construct(
        private readonly AccountRepository $accounts,
        private readonly PasswordHasher $passwords,
        private readonly TotpService $totp,
        private readonly RecoveryCodeService $recovery,
    ) {
    }

    #[Route(path: '/api/v1/account/totp/recovery-codes', methods: ['POST'], name: 'account.totp.recovery')]
    public function handle(ServerRequestInterface $request): ResponseInterface
    {
        $user = $request->getAttribute(SessionMiddleware::USER_ATTRIBUTE);
        $userId = (string) $user['id'];
        $body = Json::body($request);

        $hash = $this->accounts->passwordHash($userId);

        if ($hash === null || ! $this->passwords->verify(Json::requireString($body, 'password', 512), $hash)) {
            throw ApiException::unauthorized('That password is not right.', 'invalid_credentials');
        }

        $secret = $this->accounts->confirmedTotp($userId);

        if ($secret === null) {
            throw ApiException::conflict('This account does not have two-factor authentication.', 'totp_not_enrolled');
        }

        $step = $this->totp->verify(
            (string) $secret['secret_encrypted'],
            Json::requireString($body, 'code', 32),
            $secret['last_accepted_step'] === null ? null : (int) $secret['last_accepted_step'],
        );

        if ($step === null) {
            throw ApiException::unauthorized('That code is not right.', 'invalid_code');
        }

        $this->accounts->recordTotpStep($userId, $step);

        $codes = $this->recovery->generate();
        $this->accounts->replaceRecoveryCodes($userId, array_map($this->recovery->hash(...), $codes));

        return Json::ok(['recovery_codes' => $codes]);
    }
}
