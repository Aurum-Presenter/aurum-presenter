<?php

declare(strict_types=1);

namespace App\Handler\Auth;

use App\Attribute\Route;
use App\Auth\PasswordResetService;
use App\Http\Json;
use App\Mail\MailQueue;
use App\Support\Env;
use Psr\Http\Message\ResponseInterface;
use Psr\Http\Message\ServerRequestInterface;
use Psr\Http\Server\RequestHandlerInterface;

/**
 * Starting a password reset.
 *
 * The answer is the same whether or not the address has an account. Telling a stranger which
 * addresses are registered is a gift to somebody building a list, and the person who really
 * owns the address finds out from their inbox.
 */
final class ForgotPasswordHandler implements RequestHandlerInterface
{
    public function __construct(
        private readonly PasswordResetService $resets,
        private readonly MailQueue $mail,
    ) {
    }

    #[Route(path: '/api/v1/auth/password/forgot', methods: ['POST'], name: 'auth.password.forgot')]
    public function handle(ServerRequestInterface $request): ResponseInterface
    {
        $email = strtolower(trim(Json::requireString(Json::body($request), 'email', 320)));
        $token = $this->resets->start($email);

        if ($token !== null) {
            $link = rtrim(Env::string('APP_URL', 'http://localhost:5173') ?? '', '/') . '/auth/reset/' . $token;

            $this->mail->enqueue(
                $email,
                'Reset your Aurum password',
                sprintf(
                    '<p>Somebody asked to reset the password for this account.</p><p><a href="%s">Choose a new password</a></p><p>The link works once and expires in an hour. If this was not you, nothing has changed.</p>',
                    htmlspecialchars($link),
                ),
                sprintf(
                    "Somebody asked to reset the password for this account.\n\n%s\n\nThe link works once and expires in an hour. If this was not you, nothing has changed.\n",
                    $link,
                ),
            );
        }

        return Json::ok(['sent' => true]);
    }
}
