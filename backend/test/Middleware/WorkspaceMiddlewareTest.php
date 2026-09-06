<?php

declare(strict_types=1);

namespace AppTest\Middleware;

use App\Database\ConnectionFactory;
use App\Database\Migrator;
use App\Database\WorkspaceDatabase;
use App\Enum\WorkspaceRole;
use App\Http\ApiException;
use App\Http\RouteOptions;
use App\Middleware\SessionMiddleware;
use App\Middleware\WorkspaceMiddleware;
use App\Support\Clock;
use App\Workspace\WorkspaceRepository;
use AppTest\ControlTestCase;
use Doctrine\DBAL\Connection;
use Laminas\Diactoros\Response\EmptyResponse;
use Laminas\Diactoros\ServerRequest;
use Psr\Http\Message\ResponseInterface;
use Psr\Http\Message\ServerRequestInterface;
use Psr\Http\Server\RequestHandlerInterface;

/**
 * What the middleware hands a handler.
 *
 * The route parameter is part of that: nine handlers read the workspace id from it — the object
 * key a sheet is stored under, the workspace an invitation belongs to, the workspace whose
 * members are listed. Writing the workspace *row* over that attribute left every one of them
 * reading an array where an id was expected, which is how sheet files ended up under a key
 * called "Array" and the members list came back empty.
 */
final class WorkspaceMiddlewareTest extends ControlTestCase
{
    private WorkspaceMiddleware $middleware;
    private WorkspaceRepository $workspaces;
    private string $userId;
    private string $workspaceId;

    protected function setUp(): void
    {
        parent::setUp();

        mkdir($this->directory . '/workspace', 0o775, true);

        $this->workspaces = new WorkspaceRepository($this->control, new Clock());
        $this->userId = $this->makeUser('kate@example.com');
        $this->workspaceId = $this->workspaces->create($this->userId, 'The Anchor');

        $this->middleware = new WorkspaceMiddleware(
            $this->workspaces,
            new WorkspaceDatabase(
                new ConnectionFactory(),
                new Migrator(dirname(__DIR__, 2) . '/migrations/workspace'),
                $this->directory . '/workspace',
                true,
            ),
        );
    }

    protected function tearDown(): void
    {
        foreach (glob($this->directory . '/workspace/*') ?: [] as $file) {
            unlink($file);
        }

        @rmdir($this->directory . '/workspace');

        parent::tearDown();
    }

    public function testTheRouteParameterSurvivesForTheHandlerThatNeedsIt(): void
    {
        $seen = null;

        $this->middleware->process($this->request(), $this->capture($seen));

        self::assertSame(
            $this->workspaceId,
            $seen?->getAttribute(RouteOptions::WORKSPACE_PARAM),
            'A handler must still be able to read the workspace id from the route.',
        );
    }

    public function testTheWorkspaceRoleAndConnectionAreAttached(): void
    {
        $seen = null;

        $this->middleware->process($this->request(), $this->capture($seen));

        self::assertSame('The Anchor', $seen?->getAttribute(WorkspaceMiddleware::WORKSPACE_ATTRIBUTE)['name']);
        self::assertSame(WorkspaceRole::Owner, $seen?->getAttribute(WorkspaceMiddleware::ROLE_ATTRIBUTE));
        self::assertInstanceOf(Connection::class, $seen?->getAttribute(WorkspaceMiddleware::CONNECTION_ATTRIBUTE));
    }

    /** A non-member gets the same answer as somebody asking about a workspace that never existed. */
    public function testANonMemberIsToldItDoesNotExist(): void
    {
        $stranger = $this->makeUser('stranger@example.com');

        $this->expectException(ApiException::class);
        $this->expectExceptionMessage('No such workspace.');

        $this->middleware->process($this->request($stranger), $this->capture($ignored));
    }

    /**
     * The change request asks for more than a 403: on a denied path the workspace file must
     * never be opened at all. Membership is checked in the control database, so a request for
     * somebody else's workspace leaves no file behind — which is also what stops a stranger
     * creating workspace files by guessing ids.
     */
    public function testADeniedRequestNeverOpensAWorkspaceFile(): void
    {
        $stranger = $this->makeUser('stranger@example.com');
        $unknown = \App\Support\Uuid::generate();

        $request = (new ServerRequest())
            ->withAttribute(RouteOptions::WORKSPACE_PARAM, $unknown)
            ->withAttribute(SessionMiddleware::USER_ATTRIBUTE, ['id' => $stranger]);

        try {
            $this->middleware->process($request, $this->capture($ignored));
            self::fail('A workspace this account is not in must not be reachable.');
        } catch (ApiException) {
            // The refusal is the point; what is on disk afterwards is what this test is about.
        }

        self::assertSame([], glob($this->directory . '/workspace/' . $unknown . '*') ?: []);
    }

    public function testARouteWithNoWorkspaceParameterPassesStraightThrough(): void
    {
        $seen = null;

        $request = (new ServerRequest())->withAttribute(SessionMiddleware::USER_ATTRIBUTE, ['id' => $this->userId]);

        $this->middleware->process($request, $this->capture($seen));

        self::assertNull($seen?->getAttribute(WorkspaceMiddleware::CONNECTION_ATTRIBUTE));
    }

    private function request(?string $userId = null): ServerRequestInterface
    {
        return (new ServerRequest())
            ->withAttribute(RouteOptions::WORKSPACE_PARAM, $this->workspaceId)
            ->withAttribute(SessionMiddleware::USER_ATTRIBUTE, ['id' => $userId ?? $this->userId]);
    }

    private function capture(?ServerRequestInterface &$seen): RequestHandlerInterface
    {
        return new class ($seen) implements RequestHandlerInterface {
            public function __construct(private ?ServerRequestInterface &$seen)
            {
            }

            public function handle(ServerRequestInterface $request): ResponseInterface
            {
                $this->seen = $request;

                return new EmptyResponse();
            }
        };
    }
}
