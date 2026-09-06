<?php

declare(strict_types=1);

namespace App\Handler\Account;

use App\Account\AccountRepository;
use App\Attribute\Route;
use App\Auth\AccountService;
use App\Auth\PasswordHasher;
use App\Auth\TotpService;
use App\Http\ApiException;
use App\Http\Json;
use App\Middleware\SessionMiddleware;
use Psr\Http\Message\ResponseInterface;
use Psr\Http\Message\ServerRequestInterface;
use Psr\Http\Server\RequestHandlerInterface;

/**
 * Disabling requires the current password AND a valid code, and is refused outright while the
 * account owns a band workspace.
 */
final class TotpDisableHandler implements RequestHandlerInterface
{
    public function __construct(
        private readonly AccountRepository $accounts,
        private readonly AccountService $service,
        private readonly PasswordHasher $hasher,
        private readonly TotpService $totp,
    ) {
    }

    #[Route(path: '/api/v1/account/totp', methods: ['DELETE'], name: 'account.totp.disable')]
    public function handle(ServerRequestInterface $request): ResponseInterface
    {
        $user = $request->getAttribute(SessionMiddleware::USER_ATTRIBUTE);
        $userId = (string) $user['id'];

        if ($this->service->requiresTotp($userId)) {
            throw ApiException::forbidden(
                'You own a workspace, so two-factor cannot be turned off. Transfer ownership first.',
                'totp_required_for_owner'
            );
        }

        $body = Json::body($request);
        $hash = $this->accounts->passwordHash($userId);

        if ($hash === null || ! $this->hasher->verify(Json::requireString($body, 'password', 512), $hash)) {
            throw ApiException::unauthorized('Password is incorrect.', 'invalid_credentials');
        }

        $secret = $this->accounts->confirmedTotp($userId);

        if ($secret === null) {
            throw ApiException::conflict('Two-factor is not enabled.', 'not_enrolled');
        }

        $step = $this->totp->verify(
            (string) $secret['secret_encrypted'],
            Json::requireString($body, 'code', 32),
            $secret['last_accepted_step'] === null ? null : (int) $secret['last_accepted_step'],
        );

        if ($step === null) {
            throw ApiException::unprocessable('That code is not valid.', [], 'invalid_totp');
        }

        $this->accounts->removeTotp($userId);

        return Json::ok(['enrolled' => false]);
    }
}
