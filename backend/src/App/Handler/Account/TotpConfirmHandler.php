<?php

declare(strict_types=1);

namespace App\Handler\Account;

use App\Account\AccountRepository;
use App\Attribute\Route;
use App\Auth\RecoveryCodeService;
use App\Auth\TotpService;
use App\Http\ApiException;
use App\Http\Json;
use App\Middleware\SessionMiddleware;
use Psr\Http\Message\ResponseInterface;
use Psr\Http\Message\ServerRequestInterface;
use Psr\Http\Server\RequestHandlerInterface;

final class TotpConfirmHandler implements RequestHandlerInterface
{
    public function __construct(
        private readonly AccountRepository $accounts,
        private readonly TotpService $totp,
        private readonly RecoveryCodeService $recovery,
    ) {
    }

    #[Route(path: '/api/v1/account/totp/confirm', methods: ['POST'], name: 'account.totp.confirm')]
    public function handle(ServerRequestInterface $request): ResponseInterface
    {
        $user = $request->getAttribute(SessionMiddleware::USER_ATTRIBUTE);
        $userId = (string) $user['id'];

        $pending = $this->accounts->pendingTotp($userId);

        if ($pending === null) {
            throw ApiException::conflict('There is no enrolment in progress.', 'no_enrolment');
        }

        if ($pending['confirmed_at'] !== null) {
            throw ApiException::conflict('Two-factor is already enabled.', 'already_enrolled');
        }

        $step = $this->totp->verify(
            (string) $pending['secret_encrypted'],
            Json::requireString(Json::body($request), 'code', 32),
            null,
        );

        if ($step === null) {
            throw ApiException::unprocessable('That code is not valid.', [], 'invalid_totp');
        }

        $this->accounts->confirmTotp($userId, $step);

        // Shown exactly once. Stored only as hashes, so they cannot be re-displayed later.
        $codes = $this->recovery->generate();
        $this->accounts->replaceRecoveryCodes($userId, array_map($this->recovery->hash(...), $codes));

        return Json::ok([
            'enrolled'       => true,
            'recovery_codes' => $codes,
            'notice'         => 'These codes are shown once. Store them somewhere safe.',
        ]);
    }
}
