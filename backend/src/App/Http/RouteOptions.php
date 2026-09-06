<?php

declare(strict_types=1);

namespace App\Http;

/**
 * Keys understood inside `#[Route(options: [...])]`.
 *
 * Authorization is declared here, on the route, and resolved before dispatch — so a handler
 * that runs is already authorized and must not re-check.
 */
final class RouteOptions
{
    /** PermissionEnum required to reach the handler. Implies a workspace-scoped route. */
    public const string PERMISSION = 'permission';

    /** True for endpoints reachable without a session: register, login, invite acceptance. */
    public const string PUBLIC = 'public';

    /** Route parameter that names the workspace. */
    public const string WORKSPACE_PARAM = 'workspace';
}
