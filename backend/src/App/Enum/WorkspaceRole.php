<?php

declare(strict_types=1);

namespace App\Enum;

/**
 * A user's role *within one workspace*. Read from the membership row in control.sqlite and
 * never from a client claim (workspaces feature, business rule 6).
 */
enum WorkspaceRole: string
{
    case Owner = 'owner';
    case Editor = 'editor';
    case Viewer = 'viewer';

    public function canBeDemoted(): bool
    {
        return $this !== self::Owner;
    }
}
