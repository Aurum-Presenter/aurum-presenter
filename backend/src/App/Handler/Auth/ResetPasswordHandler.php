<?php

declare(strict_types=1);

namespace App\Handler\Auth;

use App\Attribute\Route;
use App\Auth\AccountService;
use App\Auth\PasswordResetService;
use App\Http\ApiException;
use App\Http\Json;
use Psr\Http\Message\ResponseInterface;
use Psr\Http\Message\ServerRequestInterface;
use Psr\Http\Server\RequestHandlerInterface;

/**
 * Finishing a password reset. Every existing session is revoked by the service, so a device
 * somebody else is holding stops working the moment the real owner sets a new password.
 */
final class ResetPasswordHandler implements RequestHandlerInterface
{
    public function __construct(
        private readonly PasswordResetService $resets,
        private readonly AccountService $accounts,
    ) {
    }

    #[Route(path: '/api/v1/auth/password/reset', methods: ['POST'], name: 'auth.password.reset')]
    public function handle(ServerRequestInterface $request): ResponseInterface
    {
        $body = Json::body($request);
        $token = Json::requireString($body, 'token', 128);
        $password = Json::requireString($body, 'password', 512);

        $this->accounts->assertPasswordAcceptable($password);

        if ($this->resets->complete($token, $password) === null) {
            throw ApiException::conflict(
                'That reset link has expired or has already been used. Ask for another.',
                'reset_invalid',
            );
        }

        return Json::ok(['reset' => true]);
    }
}
