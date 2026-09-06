<?php

declare(strict_types=1);

namespace App\Auth;

use App\Account\SessionRepository;
use Psr\Http\Message\ResponseInterface;
use Psr\Http\Message\ServerRequestInterface;

/**
 * Mints the token pair and attaches the refresh cookie.
 *
 * The split matters: the access token goes in the JSON body because the sync engine sends it as
 * an Authorization header from a Web Worker, while the refresh token goes in an httpOnly cookie
 * where no script — including a successful XSS payload — can read it.
 */
final class SessionIssuer
{
    public const string COOKIE_NAME = 'aurum_refresh';

    public function __construct(
        private readonly TokenService $tokens,
        private readonly SessionRepository $sessions,
        private readonly string $cookiePath = '/api/v1/auth',
        private readonly bool $cookieSecure = true,
    ) {
    }

    /**
     * @return array{
     *     body: array{access_token: string, token_type: string, expires_in: int},
     *     cookie: string,
     *     session_id: string,
     *     family_id: string
     * }
     */
    public function issue(string $userId, ServerRequestInterface $request, ?string $familyId = null): array
    {
        $refreshToken = $this->tokens->generateRefreshToken();

        $session = $this->sessions->open(
            $userId,
            $this->tokens->hashRefreshToken($refreshToken),
            $this->tokens->refreshExpiresAt(),
            $request->getHeaderLine('User-Agent') ?: null,
            $familyId,
        );

        return [
            'body' => [
                'access_token' => $this->tokens->issueAccessToken($userId, $session['id']),
                'token_type'   => 'Bearer',
                'expires_in'   => $this->tokens->accessTtlSeconds(),
            ],
            'cookie' => $this->cookie($refreshToken, $this->tokens->refreshTtlSeconds()),
            'session_id' => $session['id'],
            'family_id'  => $session['family_id'],
        ];
    }

    public function attach(ResponseInterface $response, string $cookie): ResponseInterface
    {
        return $response->withAddedHeader('Set-Cookie', $cookie);
    }

    public function clearCookie(): string
    {
        return $this->cookie('', 0);
    }

    public function readCookie(ServerRequestInterface $request): ?string
    {
        $value = $request->getCookieParams()[self::COOKIE_NAME] ?? null;

        return is_string($value) && $value !== '' ? $value : null;
    }

    private function cookie(string $value, int $maxAge): string
    {
        $parts = [
            self::COOKIE_NAME . '=' . $value,
            'Path=' . $this->cookiePath,
            'Max-Age=' . $maxAge,
            'HttpOnly',
            'SameSite=Lax',
        ];

        if ($this->cookieSecure) {
            $parts[] = 'Secure';
        }

        return implode('; ', $parts);
    }
}
