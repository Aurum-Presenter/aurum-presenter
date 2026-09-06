<?php

declare(strict_types=1);

namespace App\Command;

use App\Account\SessionRepository;
use Doctrine\DBAL\Connection;
use App\Database\WorkspaceDatabase;
use App\Storage\AssetKey;
use App\Storage\ObjectStore;
use App\Storage\OrphanSweep;
use App\Storage\SheetKey;
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

    /** Long enough that an upload in flight is never mistaken for an orphan. */
    private const int ORPHAN_GRACE_SECONDS = 30 * 86400;

    public function __construct(
        private readonly WorkspaceDatabase $workspaces,
        private readonly SessionRepository $sessions,
        private readonly ObjectStore $store,
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
        $totals = ['ops' => 0, 'tombstones' => 0, 'conflicts' => 0, 'objects' => 0];

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

            // After the rows have gone, not before: what is still referenced is what is left.
            $totals['objects'] += $this->purgeObjects($id, $db);
        }

        $io->success(sprintf(
            'Purged %d applied ops, %d tombstones, %d conflict records, %d stored files.',
            $totals['ops'],
            $totals['tombstones'],
            $totals['conflicts'],
            $totals['objects'],
        ));

        return Command::SUCCESS;
    }

    /**
     * Files nothing points at any more: the previous version of a replaced sheet, and the
     * sheets and backgrounds of rows that have just been purged for good.
     *
     * A replace deliberately leaves the old object alone so that a device still holding its URL
     * keeps receiving the bytes it cached (s3 business rule); this is where it finally goes, on
     * the same thirty-day horizon as a tombstone. Anything written inside that window is left
     * alone whatever the database says — an upload can reach the store before its row reaches
     * the server, and deleting it would lose a file nobody has a second copy of.
     */
    private function purgeObjects(string $workspaceId, Connection $db): int
    {
        $referenced = array_map(
            'strtolower',
            [
                ...$db->fetchFirstColumn('SELECT DISTINCT sha256 FROM sheets WHERE sha256 IS NOT NULL'),
                ...$db->fetchFirstColumn(
                    "SELECT DISTINCT background_value FROM presenter_themes WHERE background_kind = 'image'"
                ),
            ],
        );

        $written = time() - self::ORPHAN_GRACE_SECONDS;
        $removed = 0;

        foreach ([SheetKey::prefixFor($workspaceId), AssetKey::prefixFor($workspaceId)] as $prefix) {
            foreach (OrphanSweep::orphans($this->store->listPrefix($prefix), $referenced, $written) as $key) {
                $this->store->delete($key);
                $removed++;
            }
        }

        return $removed;
    }
}
