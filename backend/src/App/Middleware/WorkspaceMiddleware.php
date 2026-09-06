<?php

declare(strict_types=1);

namespace App\Middleware;

use App\Database\WorkspaceDatabase;
use App\Http\ApiException;
use App\Http\RouteOptions;
use App\Support\Uuid;
use App\Workspace\WorkspaceRepository;
use Psr\Http\Message\ResponseInterface;
use Psr\Http\Message\ServerRequestInterface;
use Psr\Http\Server\MiddlewareInterface;
use Psr\Http\Server\RequestHandlerInterface;

/**
 * Resolves the {workspace} route parameter to a verified membership, and only then opens that
 * workspace's database file.
 *
 * This is the structural half of what replaced row-level security. RLS made every query carry a
 * predicate that could be forgotten; here, a handler that was not granted the workspace is never
 * handed a connection to it, so there is no query to get wrong. The connection is attached to
 * the request rather than resolved from the container for the same reason — there is no way to
 * ask for "some workspace database" without having passed through here.
 */
final class WorkspaceMiddleware implements MiddlewareInterface
{
    /**
     * Deliberately NOT 'workspace': that is the route parameter's own attribute name, and
     * writing the workspace row over it left every handler downstream reading an array where it
     * expected an id — which is how sheet objects ended up under a key called "Array".
     */
    public const string WORKSPACE_ATTRIBUTE = 'workspace.record';
    public const string ROLE_ATTRIBUTE = 'workspace.role';
    public const string CONNECTION_ATTRIBUTE = 'workspace.connection';

    public function __construct(
        private readonly WorkspaceRepository $workspaces,
        private readonly WorkspaceDatabase $databases,
    ) {
    }

    public function process(ServerRequestInterface $request, RequestHandlerInterface $handler): ResponseInterface
    {
        $workspaceId = $request->getAttribute(RouteOptions::WORKSPACE_PARAM);

        // Not a workspace-scoped route; nothing to resolve.
        if (! is_string($workspaceId) || $workspaceId === '') {
            return $handler->handle($request);
        }

        if (! Uuid::isValid($workspaceId)) {
            throw ApiException::notFound('No such workspace.', 'workspace_not_found');
        }

        $user = $request->getAttribute(SessionMiddleware::USER_ATTRIBUTE);

        if (! is_array($user)) {
            throw ApiException::unauthorized();
        }

        $role = $this->workspaces->roleOf((string) $user['id'], $workspaceId);

        if ($role === null) {
            // Deliberately 404, not 403: whether a workspace exists is itself information a
            // non-member should not be able to probe for.
            throw ApiException::notFound('No such workspace.', 'workspace_not_found');
        }

        $workspace = $this->workspaces->find($workspaceId);

        if ($workspace === null) {
            throw ApiException::notFound('No such workspace.', 'workspace_not_found');
        }

        return $handler->handle(
            $request
                ->withAttribute(self::WORKSPACE_ATTRIBUTE, $workspace)
                ->withAttribute(self::ROLE_ATTRIBUTE, $role)
                ->withAttribute(self::CONNECTION_ATTRIBUTE, $this->databases->open($workspaceId))
        );
    }
}
