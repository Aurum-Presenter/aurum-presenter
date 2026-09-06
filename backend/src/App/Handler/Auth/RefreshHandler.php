<?php

declare(strict_types=1);

namespace App\Handler\Auth;

use App\Account\SessionRepository;
use App\Attribute\Route;
use App\Auth\SessionIssuer;
use App\Auth\TokenService;
use App\Http\ApiException;
use App\Http\Json;
use App\Http\RouteOptions;
use Psr\Http\Message\ResponseInterface;
use Psr\Http\Message\ServerRequestInterface;
use Psr\Http\Server\RequestHandlerInterface;

/**
 * Rotating refresh with reuse detection.
 *
 * The token is single-use. If one is presented that has already been rotated away, the only
 * safe conclusion is that a copy is circulating — so every session in that rotation family is
 * revoked and both the legitimate user and whoever stole the cookie must sign in again.
 */
final class RefreshHandler implements RequestHandlerInterface
{
    public function __construct(
        private readonly TokenService $tokens,
        private readonly SessionRepository $sessions,
        private readonly SessionIssuer $issuer,
    ) {
    }

    #[Route(
        path: '/api/v1/auth/refresh',
        methods: ['POST'],
        name: 'auth.refresh',
        options: [RouteOptions::PUBLIC => true],
    )]
    public function handle(ServerRequestInterface $request): ResponseInterface
    {
        $token = $this->issuer->readCookie($request);

        if ($token === null) {
            throw ApiException::unauthorized('No refresh token.', 'missing_refresh_token');
        }

        $session = $this->sessions->findByRefreshHash($this->tokens->hashRefreshToken($token));

        if ($session === null) {
            throw ApiException::unauthorized('Refresh token is not recognised.', 'invalid_refresh_token');
        }

        if ($session['revoked_at'] !== null) {
            $this->sessions->revokeFamily((string) $session['family_id']);

            throw ApiException::unauthorized(
                'This session was already used and has been revoked everywhere.',
                'refresh_token_reused'
            );
        }

        if (! $this->sessions->isUsable($session)) {
            throw ApiException::unauthorized('This session has expired.', 'session_expired');
        }

        $issued = $this->issuer->issue(
            (string) $session['user_id'],
            $request,
            (string) $session['family_id'],
        );

        $this->sessions->markReplaced((string) $session['id'], $issued['session_id']);

        return $this->issuer->attach(Json::ok($issued['body']), $issued['cookie']);
    }
}
