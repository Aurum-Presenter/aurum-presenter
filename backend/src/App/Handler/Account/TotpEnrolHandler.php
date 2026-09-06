<?php

declare(strict_types=1);

namespace App\Handler\Account;

use App\Account\AccountRepository;
use App\Attribute\Route;
use App\Auth\TotpService;
use App\Http\Json;
use App\Middleware\SessionMiddleware;
use Psr\Http\Message\ResponseInterface;
use Psr\Http\Message\ServerRequestInterface;
use Psr\Http\Server\RequestHandlerInterface;

/**
 * Begins enrolment. The secret is staged but NOT armed: `confirmed_at` stays null until a valid
 * code proves the authenticator actually holds it. An enrolment abandoned at this point leaves
 * the account exactly as it was — which is what stops a half-finished setup locking anyone out.
 */
final class TotpEnrolHandler implements RequestHandlerInterface
{
    public function __construct(
        private readonly AccountRepository $accounts,
        private readonly TotpService $totp,
    ) {
    }

    #[Route(path: '/api/v1/account/totp/enrol', methods: ['POST'], name: 'account.totp.enrol')]
    public function handle(ServerRequestInterface $request): ResponseInterface
    {
        $user = $request->getAttribute(SessionMiddleware::USER_ATTRIBUTE);
        $userId = (string) $user['id'];

        $secret = $this->totp->generateSecret();
        $this->accounts->stageTotpSecret($userId, $this->totp->encryptSecret($secret));

        return Json::ok([
            'secret'           => $secret,
            'provisioning_uri' => $this->totp->provisioningUri($secret, (string) $user['email']),
            'digits'           => TotpService::DIGITS,
            'period'           => TotpService::PERIOD,
        ]);
    }
}
