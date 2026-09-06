<?php

declare(strict_types=1);

namespace App\Enum;

/**
 * Every permission in the system. Deliberately few: because each workspace is its own database
 * file, a permission never needs to name *which* workspace it applies to — WorkspaceMiddleware
 * has already decided that by opening (or refusing to open) the file.
 */
enum PermissionEnum: string
{
    /** Read any content in the workspace, and write per-user preferences. */
    case WorkspaceRead = 'workspace.read';

    /** Create, edit and delete shared content: folders, songs, arrangements, sheets, sets. */
    case WorkspaceWrite = 'workspace.write';

    /** Manage members, transfer ownership, delete the workspace. */
    case WorkspaceManage = 'workspace.manage';

    /** Run a live session. Explicitly granted to viewers — see the overview's role table. */
    case SessionRun = 'session.run';
}
