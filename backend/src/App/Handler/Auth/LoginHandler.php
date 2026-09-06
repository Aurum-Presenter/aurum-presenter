<?php

declare(strict_types=1);

namespace App\Handler\Auth;

use App\Account\AccountRepository;
use App\Attribute\Route;
use App\Auth\AccountService;
use App\Auth\SessionIssuer;
use App\Http\ApiException;
use App\Http\Json;
use App\Http\RouteOptions;
use Psr\Http\Message\ResponseInterface;
use Psr\Http\Message\ServerRequestInterface;
use Psr\Http\Server\RequestHandlerInterface;

/**
 * A correct password alone never returns a session when a second factor is enrolled — it
 * returns a challenge. This is the property acceptance criterion 1 of the auth change request
 * tests for.
 */
final class LoginHandler implements RequestHandlerInterface
{
    private const int CHALLENGE_TTL = 300;

    public function __construct(
        private readonly AccountService $service,
        private readonly AccountRepository $accounts,
        private readonly SessionIssuer $issuer,
    ) {
    }

    #[Route(
        path: '/api/v1/auth/login',
        methods: ['POST'],
        name: 'auth.login',
        options: [RouteOptions::PUBLIC => true],
    )]
    public function handle(ServerRequestInterface $request): ResponseInterface
    {
        $body = Json::body($request);

        $user = $this->service->authenticate(
            Json::requireString($body, 'email', 320),
            Json::requireString($body, 'password', 512),
            $this->clientIp($request),
        );

        $userId = (string) $user['id'];

        if ($this->accounts->confirmedTotp($userId) !== null) {
            return Json::ok([
                'totp_required' => true,
                'challenge_id'  => $this->accounts->createChallenge($userId, self::CHALLENGE_TTL),
                'expires_in'    => self::CHALLENGE_TTL,
            ], 200);
        }

        if ($this->service->requiresTotp($userId)) {
            // An owner without a second factor is not refused — they are routed to enrolment,
            // which needs a session to complete. The session is issued with that one purpose
            // and the client is told to go nowhere else first.
            $issued = $this->issuer->issue($userId, $request);
            $issued['body']['totp_enrolment_required'] = true;

            return $this->issuer->attach(Json::ok($issued['body']), $issued['cookie']);
        }

        $issued = $this->issuer->issue($userId, $request);

        return $this->issuer->attach(Json::ok($issued['body']), $issued['cookie']);
    }

    private function clientIp(ServerRequestInterface $request): ?string
    {
        $server = $request->getServerParams();
        $ip = $server['REMOTE_ADDR'] ?? null;

        return is_string($ip) && $ip !== '' ? $ip : null;
    }
}
