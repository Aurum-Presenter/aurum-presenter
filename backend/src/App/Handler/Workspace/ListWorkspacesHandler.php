<?php

declare(strict_types=1);

namespace App\Handler\Workspace;

use App\Attribute\Route;
use App\Http\Json;
use App\Middleware\SessionMiddleware;
use App\Workspace\WorkspaceRepository;
use Psr\Http\Message\ResponseInterface;
use Psr\Http\Message\ServerRequestInterface;
use Psr\Http\Server\RequestHandlerInterface;

final class ListWorkspacesHandler implements RequestHandlerInterface
{
    public function __construct(private readonly WorkspaceRepository $workspaces)
    {
    }

    #[Route(path: '/api/v1/workspaces', methods: ['GET'], name: 'workspaces.list')]
    public function handle(ServerRequestInterface $request): ResponseInterface
    {
        $user = $request->getAttribute(SessionMiddleware::USER_ATTRIBUTE);

        return Json::ok(['workspaces' => $this->workspaces->forUser((string) $user['id'])]);
    }
}
