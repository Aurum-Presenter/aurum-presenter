<?php

declare(strict_types=1);

namespace App\Storage;

/**
 * Which stored files nothing points at any more.
 *
 * A replace deliberately leaves the old object where it is, so a device still holding its URL
 * keeps receiving the bytes it cached; this is the rule that decides when it finally goes. Two
 * things stop a file being swept: a row still names its hash, or it was written recently — an
 * upload can reach the object store before its row reaches the server, and a file the store has
 * but the database does not know about yet may be the only copy in existence.
 */
final class OrphanSweep
{
    /**
     * @param list<array{key: string, size: int, modified: int}> $objects
     * @param list<string>                                       $referenced hashes still named by a row
     *
     * @return list<string> keys to delete
     */
    public static function orphans(array $objects, array $referenced, int $writtenBefore): array
    {
        $keep = array_flip(array_map('strtolower', $referenced));
        $orphans = [];

        foreach ($objects as $object) {
            $hash = strtolower(pathinfo($object['key'], PATHINFO_FILENAME));

            if (isset($keep[$hash]) || $object['modified'] >= $writtenBefore) {
                continue;
            }

            $orphans[] = $object['key'];
        }

        return $orphans;
    }
}
