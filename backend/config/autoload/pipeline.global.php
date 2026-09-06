<?php

declare(strict_types=1);

use App\Middleware\ApiErrorHandlerMiddleware;
use App\Middleware\AuthorizationMiddleware;
use App\Middleware\CorsMiddleware;
use App\Middleware\SessionMiddleware;
use App\Middleware\WorkspaceMiddleware;
use Mezzio\Handler\NotFoundHandler;
use Mezzio\Helper\BodyParams\BodyParamsMiddleware;
use Mezzio\Helper\ServerUrlMiddleware;
use Mezzio\Router\Middleware\DispatchMiddleware;
use Mezzio\Router\Middleware\ImplicitHeadMiddleware;
use Mezzio\Router\Middleware\ImplicitOptionsMiddleware;
use Mezzio\Router\Middleware\MethodNotAllowedMiddleware;
use Mezzio\Router\Middleware\RouteMiddleware;

/**
 * Order is load-bearing:
 *
 *  - CorsMiddleware is outermost, above the error handler, so that an *error* response carries
 *    the CORS headers too. Inside the handler it would only ever decorate a response that came
 *    back normally: an exception unwinds straight past it to the error handler, and the browser
 *    then refuses to let the app read the 401 it was given — which is exactly the response the
 *    client needs most, because it is the one that triggers a token refresh.
 *  - ApiErrorHandlerMiddleware is next, so every failure below it becomes a JSON envelope.
 *  - The three authorization middlewares all sit AFTER RouteMiddleware, because each reads
 *    something the router produced (route options, or the {workspace} parameter), and BEFORE
 *    DispatchMiddleware, so an unauthorized request never reaches a handler.
 *  - Within them the order is Session -> Workspace -> Authorization: the workspace lookup needs
 *    the user id, and the permission check needs the role that lookup returned.
 */
return [
    'middleware_pipeline' => [
        ['middleware' => CorsMiddleware::class],
        ['middleware' => ApiErrorHandlerMiddleware::class],
        ['middleware' => ServerUrlMiddleware::class],
        ['middleware' => BodyParamsMiddleware::class],
        ['middleware' => RouteMiddleware::class],
        ['middleware' => SessionMiddleware::class],
        ['middleware' => WorkspaceMiddleware::class],
        ['middleware' => AuthorizationMiddleware::class],
        ['middleware' => ImplicitHeadMiddleware::class],
        ['middleware' => ImplicitOptionsMiddleware::class],
        ['middleware' => MethodNotAllowedMiddleware::class],
        ['middleware' => DispatchMiddleware::class],
        ['middleware' => NotFoundHandler::class],
    ],
];
