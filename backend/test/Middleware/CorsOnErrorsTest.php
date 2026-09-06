<?php

declare(strict_types=1);

namespace AppTest\Middleware;

use App\Http\ApiException;
use App\Middleware\ApiErrorHandlerMiddleware;
use App\Middleware\CorsMiddleware;
use Laminas\Diactoros\Response\JsonResponse;
use Laminas\Diactoros\ServerRequest;
use PHPUnit\Framework\TestCase;
use Psr\Http\Message\ResponseInterface;
use Psr\Http\Message\ServerRequestInterface;
use Psr\Http\Server\RequestHandlerInterface;

/**
 * The pipeline order that makes an error response readable by the app.
 *
 * With CORS inside the error handler, an exception unwinds straight past the CORS middleware:
 * the browser is then handed a 401 with no `Access-Control-Allow-Origin`, refuses to let the
 * page read it, and the client sees a network failure instead of `token_expired` — the one
 * response it must be able to act on. So CORS is outermost, and this test says why.
 */
final class CorsOnErrorsTest extends TestCase
{
    private const string ORIGIN = 'http://localhost:5173';

    public function testAThrownApiExceptionStillCarriesTheCorsHeaders(): void
    {
        $response = $this->pipeline(static function (): never {
            throw ApiException::unauthorized('The access token has expired.', 'token_expired');
        });

        self::assertSame(401, $response->getStatusCode());
        self::assertSame(self::ORIGIN, $response->getHeaderLine('Access-Control-Allow-Origin'));
        self::assertSame('true', $response->getHeaderLine('Access-Control-Allow-Credentials'));
        self::assertStringContainsString('token_expired', (string) $response->getBody());
    }

    public function testAnUnexpectedFailureAlsoCarriesThem(): void
    {
        $response = $this->pipeline(static function (): never {
            throw new \RuntimeException('the database fell over');
        });

        self::assertSame(500, $response->getStatusCode());
        self::assertSame(self::ORIGIN, $response->getHeaderLine('Access-Control-Allow-Origin'));
    }

    public function testASuccessfulResponseIsUnaffected(): void
    {
        $response = $this->pipeline(static fn (): ResponseInterface => new JsonResponse(['ok' => true]));

        self::assertSame(200, $response->getStatusCode());
        self::assertSame(self::ORIGIN, $response->getHeaderLine('Access-Control-Allow-Origin'));
    }

    public function testAnOriginThatIsNotAllowedGetsNoHeaderEvenOnAnError(): void
    {
        $request = (new ServerRequest())->withHeader('Origin', 'https://somewhere-else.example');

        $response = (new CorsMiddleware([self::ORIGIN]))->process(
            $request,
            $this->handlerFor(static function (): never {
                throw ApiException::unauthorized();
            }),
        );

        self::assertSame(401, $response->getStatusCode());
        self::assertFalse($response->hasHeader('Access-Control-Allow-Origin'));
    }

    /** @param callable(): ResponseInterface $inner */
    private function pipeline(callable $inner): ResponseInterface
    {
        $request = (new ServerRequest())->withHeader('Origin', self::ORIGIN);

        return (new CorsMiddleware([self::ORIGIN]))->process($request, $this->handlerFor($inner));
    }

    /** @param callable(): ResponseInterface $inner */
    private function handlerFor(callable $inner): RequestHandlerInterface
    {
        return new class ($inner) implements RequestHandlerInterface {
            /** @param callable(): ResponseInterface $inner */
            public function __construct(private $inner)
            {
            }

            public function handle(ServerRequestInterface $request): ResponseInterface
            {
                $errors = new ApiErrorHandlerMiddleware(false);
                $inner = $this->inner;

                return $errors->process($request, new class ($inner) implements RequestHandlerInterface {
                    /** @param callable(): ResponseInterface $inner */
                    public function __construct(private $inner)
                    {
                    }

                    public function handle(ServerRequestInterface $request): ResponseInterface
                    {
                        return ($this->inner)();
                    }
                });
            }
        };
    }
}
