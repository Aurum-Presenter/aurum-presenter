<?php

declare(strict_types=1);

namespace App\Command;

use App\Database\WorkspaceDatabase;
use Symfony\Component\Console\Command\Command;
use Symfony\Component\Console\Input\InputInterface;
use Symfony\Component\Console\Output\OutputInterface;
use Symfony\Component\Console\Style\SymfonyStyle;

final class ListWorkspacesCommand extends Command
{
    public function __construct(private readonly WorkspaceDatabase $workspaces)
    {
        parent::__construct('workspace:list');
    }

    protected function configure(): void
    {
        $this->setDescription('List every workspace database file on this host, with its size');
    }

    protected function execute(InputInterface $input, OutputInterface $output): int
    {
        $io = new SymfonyStyle($input, $output);
        $rows = [];

        foreach ($this->workspaces->all() as $id) {
            $path = $this->workspaces->pathFor($id);
            $rows[] = [$id, number_format((int) (filesize($path) / 1024)) . ' KB', $path];
        }

        if ($rows === []) {
            $io->writeln('No workspace databases yet.');

            return Command::SUCCESS;
        }

        $io->table(['Workspace', 'Size', 'Path'], $rows);

        return Command::SUCCESS;
    }
}
