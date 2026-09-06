<?php

declare(strict_types=1);

namespace App\Handler\Account;

use App\Account\AccountRepository;
use App\Attribute\Route;
use App\Auth\AccountService;
use App\Http\Json;
use App\Middleware\SessionMiddleware;
use App\Workspace\WorkspaceRepository;
use Psr\Http\Message\ResponseInterface;
use Psr\Http\Message\ServerRequestInterface;
use Psr\Http\Server\RequestHandlerInterface;

final class MeHandler implements RequestHandlerInterface
{
    public function __construct(
        private readonly AccountRepository $accounts,
        private readonly AccountService $service,
        private readonly WorkspaceRepository $workspaces,
    ) {
    }

    #[Route(path: '/api/v1/account', methods: ['GET'], name: 'account.me')]
    public function handle(ServerRequestInterface $request): ResponseInterface
    {
        $user = $request->getAttribute(SessionMiddleware::USER_ATTRIBUTE);
        $userId = (string) $user['id'];

        return Json::ok([
            'id'           => $userId,
            'email'        => $user['email'],
            'display_name' => $user['display_name'],
            'totp' => [
                'enrolled'                 => $this->accounts->confirmedTotp($userId) !== null,
                'required'                 => $this->service->requiresTotp($userId),
                'recovery_codes_remaining' => $this->accounts->remainingRecoveryCodes($userId),
            ],
            'workspaces' => $this->workspaces->forUser($userId),
        ]);
    }
}
