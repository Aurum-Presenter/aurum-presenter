<?php

declare(strict_types=1);

namespace App\Command;

use App\Database\ControlDatabase;
use App\Database\WorkspaceDatabase;
use Symfony\Component\Console\Command\Command;
use Symfony\Component\Console\Input\InputInterface;
use Symfony\Component\Console\Input\InputOption;
use Symfony\Component\Console\Output\OutputInterface;
use Symfony\Component\Console\Style\SymfonyStyle;
use Throwable;

/**
 * Applies both migration sets: control.sqlite once, then every workspace file.
 *
 * Walking every workspace is the cost of one-file-per-tenant. It is bounded by the number of
 * workspaces on the host, and each file is independent — so one failure is reported and the
 * rest still migrate, rather than the whole run aborting on a single bad file.
 */
final class MigrateCommand extends Command
{
    public function __construct(
        private readonly ControlDatabase $control,
        private readonly WorkspaceDatabase $workspaces,
    ) {
        parent::__construct('migrate');
    }

    protected function configure(): void
    {
        $this
            ->setDescription('Apply pending migrations to control.sqlite and every workspace database')
            ->addOption('control-only', null, InputOption::VALUE_NONE, 'Skip the workspace files')
            ->addOption('workspace', 'w', InputOption::VALUE_REQUIRED, 'Migrate only this workspace id');
    }

    protected function execute(InputInterface $input, OutputInterface $output): int
    {
        $io = new SymfonyStyle($input, $output);

        $io->section('Control database');
        $applied = $this->control->migrate();
        $io->writeln($applied === [] ? '  already up to date' : '  applied: ' . implode(', ', $applied));

        if ($input->getOption('control-only')) {
            return Command::SUCCESS;
        }

        $only = $input->getOption('workspace');
        $ids = is_string($only) && $only !== '' ? [$only] : $this->workspaces->all();

        $io->section(sprintf('Workspace databases (%d)', count($ids)));

        $failed = 0;
        foreach ($ids as $id) {
            try {
                $applied = $this->workspaces->migrate($id);
                $io->writeln(sprintf(
                    '  %s  %s',
                    $id,
                    $applied === [] ? 'up to date' : 'applied ' . implode(', ', $applied)
                ));
            } catch (Throwable $e) {
                $failed++;
                $io->writeln(sprintf('  <error>%s  %s</error>', $id, $e->getMessage()));
            }
        }

        if ($failed > 0) {
            $io->error(sprintf('%d workspace database(s) failed to migrate.', $failed));

            return Command::FAILURE;
        }

        $io->success('Migrations complete.');

        return Command::SUCCESS;
    }
}
