<?php

declare(strict_types=1);

namespace App\Handler\Auth;

use App\Attribute\Route;
use App\Auth\AccountService;
use App\Auth\SessionIssuer;
use App\Http\Json;
use App\Http\RouteOptions;
use Psr\Http\Message\ResponseInterface;
use Psr\Http\Message\ServerRequestInterface;
use Psr\Http\Server\RequestHandlerInterface;

final class RegisterHandler implements RequestHandlerInterface
{
    public function __construct(
        private readonly AccountService $accounts,
        private readonly SessionIssuer $issuer,
    ) {
    }

    #[Route(
        path: '/api/v1/auth/register',
        methods: ['POST'],
        name: 'auth.register',
        options: [RouteOptions::PUBLIC => true],
    )]
    public function handle(ServerRequestInterface $request): ResponseInterface
    {
        $body = Json::body($request);

        $userId = $this->accounts->register(
            Json::requireString($body, 'email', 320),
            Json::requireString($body, 'display_name', 120),
            Json::requireString($body, 'password', 512),
        );

        $issued = $this->issuer->issue($userId, $request);

        return $this->issuer->attach(Json::ok($issued['body'], 201), $issued['cookie']);
    }
}
