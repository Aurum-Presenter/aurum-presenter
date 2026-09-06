<?php

declare(strict_types=1);

namespace App\Middleware;

use App\Auth\AuthorizationService;
use App\Enum\PermissionEnum;
use App\Enum\WorkspaceRole;
use App\Http\ApiException;
use App\Http\RouteOptions;
use Mezzio\Router\Route;
use Mezzio\Router\RouteResult;
use Psr\Http\Message\ResponseInterface;
use Psr\Http\Message\ServerRequestInterface;
use Psr\Http\Server\MiddlewareInterface;
use Psr\Http\Server\RequestHandlerInterface;

/**
 * Enforces the permission declared in the matched route's options.
 *
 * Must sit after RouteMiddleware (it reads the matched route) and before DispatchMiddleware
 * (so an unauthorized request never reaches the handler).
 */
final class AuthorizationMiddleware implements MiddlewareInterface
{
    public function __construct(private readonly AuthorizationService $authorization)
    {
    }

    public function process(ServerRequestInterface $request, RequestHandlerInterface $handler): ResponseInterface
    {
        $result = $request->getAttribute(RouteResult::class);

        if (! $result instanceof RouteResult || ! ($route = $result->getMatchedRoute()) instanceof Route) {
            return $handler->handle($request);
        }

        $permission = $route->getOptions()[RouteOptions::PERMISSION] ?? null;

        if (! $permission instanceof PermissionEnum) {
            return $handler->handle($request);
        }

        $role = $request->getAttribute(WorkspaceMiddleware::ROLE_ATTRIBUTE);

        if (! $role instanceof WorkspaceRole) {
            // A route declaring a workspace permission but resolving no workspace role is a
            // wiring mistake, not a client error — fail closed and say so.
            throw ApiException::forbidden(
                'This route requires a workspace context that was not resolved.',
                'workspace_context_missing'
            );
        }

        if (! $this->authorization->isGranted($role, $permission)) {
            throw ApiException::forbidden(
                sprintf('Your role (%s) does not permit this action.', $role->value),
                'insufficient_role'
            );
        }

        return $handler->handle($request);
    }
}
