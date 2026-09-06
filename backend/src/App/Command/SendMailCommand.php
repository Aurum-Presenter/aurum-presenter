<?php

declare(strict_types=1);

namespace App\Command;

use App\Mail\MailQueue;
use App\Support\Env;
use Symfony\Component\Console\Command\Command;
use Symfony\Component\Console\Input\InputInterface;
use Symfony\Component\Console\Output\OutputInterface;
use Symfony\Component\Console\Style\SymfonyStyle;
use Symfony\Component\Mailer\Mailer;
use Symfony\Component\Mailer\Transport;

/**
 * Delivers whatever is waiting in the mail queue.
 *
 * Run from cron, or by hand in development against Mailpit. Keeping delivery out of the request
 * is what stops an invitation failing because a mail server was slow.
 */
final class SendMailCommand extends Command
{
    public function __construct(private readonly MailQueue $queue)
    {
        parent::__construct('mail:send');
    }

    protected function configure(): void
    {
        $this->setDescription('Deliver queued email (invitations, password resets)');
    }

    protected function execute(InputInterface $input, OutputInterface $output): int
    {
        $io = new SymfonyStyle($input, $output);
        $dsn = Env::string('MAIL_DSN', 'null://null') ?? 'null://null';

        $result = $this->queue->drain(
            new Mailer(Transport::fromDsn($dsn)),
            Env::string('MAIL_FROM', 'aurum@localhost') ?? 'aurum@localhost',
        );

        $io->writeln(sprintf('%d sent, %d failed', $result['sent'], $result['failed']));

        foreach ($this->queue->undelivered() as $stuck) {
            $io->warning(sprintf('Gave up on %s: %s', $stuck['recipient'], (string) $stuck['last_error']));
        }

        return Command::SUCCESS;
    }
}
