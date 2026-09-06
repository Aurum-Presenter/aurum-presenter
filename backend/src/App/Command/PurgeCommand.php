<?php

declare(strict_types=1);

namespace App\Command;

use App\Account\SessionRepository;
use App\Database\WorkspaceDatabase;
use App\Support\Clock;
use Symfony\Component\Console\Command\Command;
use Symfony\Component\Console\Input\InputInterface;
use Symfony\Component\Console\Output\OutputInterface;
use Symfony\Component\Console\Style\SymfonyStyle;

/**
 * The maintenance sweep. Two horizons, both from the sync feature: applied operation ids are
 * kept 24 hours (long enough for a retried batch after a lost response), tombstones and
 * conflict records 30 days.
 */
final class PurgeCommand extends Command
{
    private const array TOMBSTONED_TABLES = [
        'folders', 'songs', 'arrangements', 'sheets', 'annotations', 'sets', 'set_items', 'preferences',
    ];

    public function __construct(
        private readonly WorkspaceDatabase $workspaces,
        private readonly SessionRepository $sessions,
        private readonly Clock $clock,
    ) {
        parent::__construct('maintenance:purge');
    }

    protected function configure(): void
    {
        $this->setDescription('Purge applied op ids, expired sessions, tombstones and old conflicts');
    }

    protected function execute(InputInterface $input, OutputInterface $output): int
    {
        $io = new SymfonyStyle($input, $output);

        $io->writeln(sprintf('Expired sessions removed: %d', $this->sessions->purgeExpired()));

        $opCutoff = $this->clock->minusSeconds(86400);
        $tombstoneCutoff = $this->clock->minusSeconds(30 * 86400);
        $totals = ['ops' => 0, 'tombstones' => 0, 'conflicts' => 0];

        foreach ($this->workspaces->all() as $id) {
            $db = $this->workspaces->open($id);

            $totals['ops'] += $db->executeStatement('DELETE FROM applied_ops WHERE applied_at < ?', [$opCutoff]);
            $totals['conflicts'] += $db->executeStatement('DELETE FROM sync_conflicts WHERE at < ?', [$tombstoneCutoff]);

            foreach (self::TOMBSTONED_TABLES as $table) {
                $totals['tombstones'] += $db->executeStatement(
                    sprintf('DELETE FROM %s WHERE deleted_at IS NOT NULL AND deleted_at < ?', $table),
                    [$tombstoneCutoff]
                );
            }
        }

        $io->success(sprintf(
            'Purged %d applied ops, %d tombstones, %d conflict records.',
            $totals['ops'],
            $totals['tombstones'],
            $totals['conflicts'],
        ));

        return Command::SUCCESS;
    }
}
