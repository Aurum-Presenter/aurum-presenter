<?php

declare(strict_types=1);

namespace App\Middleware;

use App\Account\AccountRepository;
use App\Account\SessionRepository;
use App\Auth\TokenService;
use App\Http\ApiException;
use App\Http\RouteOptions;
use Mezzio\Router\Route;
use Mezzio\Router\RouteResult;
use Psr\Http\Message\ResponseInterface;
use Psr\Http\Message\ServerRequestInterface;
use Psr\Http\Server\MiddlewareInterface;
use Psr\Http\Server\RequestHandlerInterface;

/**
 * Resolves the bearer access token to an account, from control.sqlite.
 *
 * Runs after RouteMiddleware because whether a missing token is fatal depends on the matched
 * route's options — and before WorkspaceMiddleware, which needs the user id to look up a
 * membership.
 */
final class SessionMiddleware implements MiddlewareInterface
{
    public const string USER_ATTRIBUTE = 'auth.user';
    public const string SESSION_ATTRIBUTE = 'auth.session_id';

    public function __construct(
        private readonly TokenService $tokens,
        private readonly AccountRepository $accounts,
        private readonly SessionRepository $sessions,
    ) {
    }

    public function process(ServerRequestInterface $request, RequestHandlerInterface $handler): ResponseInterface
    {
        $isPublic = $this->routeIsPublic($request);
        $token = $this->bearerToken($request);

        if ($token === null) {
            if ($isPublic) {
                return $handler->handle($request);
            }

            throw ApiException::unauthorized('A bearer access token is required.', 'missing_token');
        }

        $claims = $this->tokens->verifyAccessToken($token);

        if ($claims === null) {
            if ($isPublic) {
                return $handler->handle($request);
            }

            // Distinguished from missing_token so the client knows to attempt a refresh rather
            // than send the user back to the sign-in screen.
            throw ApiException::unauthorized('Access token is invalid or expired.', 'token_expired');
        }

        $session = $this->sessions->findById($claims['sid']);

        if ($session === null || ! $this->sessions->isUsable($session)) {
            if ($isPublic) {
                return $handler->handle($request);
            }

            throw ApiException::unauthorized('This session has been revoked.', 'session_revoked');
        }

        $user = $this->accounts->findById($claims['sub']);

        if ($user === null) {
            throw ApiException::unauthorized('Account no longer exists.', 'account_missing');
        }

        return $handler->handle(
            $request
                ->withAttribute(self::USER_ATTRIBUTE, $user)
                ->withAttribute(self::SESSION_ATTRIBUTE, $claims['sid'])
        );
    }

    private function bearerToken(ServerRequestInterface $request): ?string
    {
        $header = $request->getHeaderLine('Authorization');

        if ($header === '' || ! preg_match('/^Bearer\s+(.+)$/i', $header, $matches)) {
            return null;
        }

        return trim($matches[1]);
    }

    private function routeIsPublic(ServerRequestInterface $request): bool
    {
        $result = $request->getAttribute(RouteResult::class);

        if (! $result instanceof RouteResult) {
            return true;
        }

        $route = $result->getMatchedRoute();

        return $route instanceof Route && ($route->getOptions()[RouteOptions::PUBLIC] ?? false) === true;
    }
}
