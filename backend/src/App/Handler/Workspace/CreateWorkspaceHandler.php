<?php

declare(strict_types=1);

namespace App\Handler\Workspace;

use App\Attribute\Route;
use App\Database\WorkspaceDatabase;
use App\Http\ApiException;
use App\Http\Json;
use App\Middleware\SessionMiddleware;
use App\Support\Uuid;
use App\Workspace\WorkspaceRepository;
use Psr\Http\Message\ResponseInterface;
use Psr\Http\Message\ServerRequestInterface;
use Psr\Http\Server\RequestHandlerInterface;

/**
 * Creating a workspace creates its database file. The optional client-supplied id is what makes
 * local-only mode work: a workspace created offline keeps the UUID its records already reference,
 * so claiming it costs no re-download.
 */
final class CreateWorkspaceHandler implements RequestHandlerInterface
{
    public function __construct(
        private readonly WorkspaceRepository $workspaces,
        private readonly WorkspaceDatabase $databases,
    ) {
    }

    #[Route(path: '/api/v1/workspaces', methods: ['POST'], name: 'workspaces.create')]
    public function handle(ServerRequestInterface $request): ResponseInterface
    {
        $user = $request->getAttribute(SessionMiddleware::USER_ATTRIBUTE);
        $body = Json::body($request);

        $id = Json::optionalString($body, 'id');

        if ($id !== null) {
            if (! Uuid::isValid($id)) {
                throw ApiException::unprocessable('"id" must be a UUID.', ['field' => 'id'], 'validation_failed');
            }

            if ($this->workspaces->find($id) !== null || $this->databases->exists($id)) {
                throw ApiException::conflict('That workspace id is already in use.', 'workspace_exists');
            }
        }

        $workspaceId = $this->workspaces->create(
            (string) $user['id'],
            Json::requireString($body, 'name', 120),
            'band',
            $id,
        );

        $this->databases->open($workspaceId);

        return Json::ok(['workspace' => $this->workspaces->find($workspaceId)], 201);
    }
}
