<?php

declare(strict_types=1);

namespace App\Handler\Auth;

use App\Account\SessionRepository;
use App\Attribute\Route;
use App\Auth\SessionIssuer;
use App\Http\Json;
use App\Middleware\SessionMiddleware;
use Psr\Http\Message\ResponseInterface;
use Psr\Http\Message\ServerRequestInterface;
use Psr\Http\Server\RequestHandlerInterface;

final class LogoutHandler implements RequestHandlerInterface
{
    public function __construct(
        private readonly SessionRepository $sessions,
        private readonly SessionIssuer $issuer,
    ) {
    }

    #[Route(path: '/api/v1/auth/logout', methods: ['POST'], name: 'auth.logout')]
    public function handle(ServerRequestInterface $request): ResponseInterface
    {
        $sessionId = $request->getAttribute(SessionMiddleware::SESSION_ATTRIBUTE);

        if (is_string($sessionId)) {
            $this->sessions->revoke($sessionId);
        }

        // Nothing local is touched here. What the device keeps in IndexedDB after sign-out is
        // the client's decision, per the workspaces feature's sign-out rule.
        return $this->issuer->attach(Json::ok(['signed_out' => true]), $this->issuer->clearCookie());
    }
}
