<?php

declare(strict_types=1);

namespace App\Mail;

use App\Database\ControlDatabase;
use App\Support\Clock;
use App\Support\Uuid;
use Symfony\Component\Mailer\MailerInterface;
use Symfony\Component\Mime\Email;

/**
 * Outgoing email, queued rather than sent inline.
 *
 * An invite must not fail because a mail server is slow, and a request must not wait on one.
 * The row is written in the same breath as the invite; a separate command delivers it, retries
 * it, and leaves the failure visible if it never goes.
 */
final class MailQueue
{
    private const int MAX_ATTEMPTS = 5;

    public function __construct(
        private readonly ControlDatabase $control,
        private readonly Clock $clock,
    ) {
    }

    public function enqueue(string $recipient, string $subject, string $html, string $text): string
    {
        $id = Uuid::generate();

        $this->control->connection()->insert('mail_queue', [
            'id'         => $id,
            'recipient'  => $recipient,
            'subject'    => $subject,
            'body_html'  => $html,
            'body_text'  => $text,
            'attempts'   => 0,
            'created_at' => $this->clock->now(),
        ]);

        return $id;
    }

    /** @return array{sent: int, failed: int} */
    public function drain(MailerInterface $mailer, string $from, int $limit = 50): array
    {
        $rows = $this->control->connection()->fetchAllAssociative(
            'SELECT * FROM mail_queue WHERE sent_at IS NULL AND attempts < ? ORDER BY created_at LIMIT ?',
            [self::MAX_ATTEMPTS, $limit],
        );

        $sent = 0;
        $failed = 0;

        foreach ($rows as $row) {
            try {
                $mailer->send(
                    (new Email())
                        ->from($from)
                        ->to((string) $row['recipient'])
                        ->subject((string) $row['subject'])
                        ->text((string) $row['body_text'])
                        ->html((string) $row['body_html']),
                );

                $this->control->connection()->update(
                    'mail_queue',
                    ['sent_at' => $this->clock->now()],
                    ['id' => $row['id']],
                );

                $sent++;
            } catch (\Throwable $e) {
                // Five attempts, then it stops trying and stays visible as undelivered rather
                // than disappearing into a log nobody reads.
                $this->control->connection()->update(
                    'mail_queue',
                    ['attempts' => (int) $row['attempts'] + 1, 'last_error' => $e->getMessage()],
                    ['id' => $row['id']],
                );

                $failed++;
            }
        }

        return ['sent' => $sent, 'failed' => $failed];
    }

    /** @return list<array<string, mixed>> messages that have given up, for the Members page. */
    public function undelivered(): array
    {
        return $this->control->connection()->fetchAllAssociative(
            'SELECT id, recipient, subject, attempts, last_error FROM mail_queue
             WHERE sent_at IS NULL AND attempts >= ?',
            [self::MAX_ATTEMPTS],
        );
    }
}
