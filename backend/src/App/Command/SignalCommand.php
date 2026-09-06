<?php

declare(strict_types=1);

namespace App\Command;

use App\Signal\SignalServer;
use Symfony\Component\Console\Command\Command;
use Symfony\Component\Console\Input\InputInterface;
use Symfony\Component\Console\Input\InputOption;
use Symfony\Component\Console\Output\OutputInterface;

/**
 * Runs the stage-pairing signalling relay.
 *
 * A separate process from the API: it holds sockets open, which is the one thing the request
 * lifecycle cannot do, and it must never be able to block an API request while it waits.
 */
final class SignalCommand extends Command
{
    public function __construct(private readonly SignalServer $server)
    {
        parent::__construct('signal:serve');
    }

    protected function configure(): void
    {
        $this
            ->setDescription('Serve the LAN stage-pairing signalling relay (SDP and ICE only)')
            ->addOption('bind', null, InputOption::VALUE_REQUIRED, 'Address to bind', '0.0.0.0')
            ->addOption('port', null, InputOption::VALUE_REQUIRED, 'Port to listen on', '8081');
    }

    protected function execute(InputInterface $input, OutputInterface $output): int
    {
        $this->server->run(
            (string) $input->getOption('bind'),
            (int) $input->getOption('port'),
            static fn (string $line) => $output->writeln($line),
        );

        return Command::SUCCESS;
    }
}
