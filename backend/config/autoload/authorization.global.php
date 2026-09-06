<?php

declare(strict_types=1);

use App\Enum\PermissionEnum;
use App\Enum\WorkspaceRole;

/**
 * The role graph that replaced row-level security.
 *
 * Note the inversion, which is the one thing in this file that is easy to get backwards: the
 * `children` key names the roles a role ABSORBS. Laminas' Role::hasPermission() recurses into
 * children, so permissions bubble *upward* — an owner listing `editor` as a child holds
 * everything an editor holds, not the other way round.
 *
 * There are no entity assertions. In the Supabase design an assertion would have answered "is
 * this record in a workspace the caller belongs to"; with one database file per workspace, that
 * question is settled before a handler ever runs.
 */
return [
    'authorization' => [
        'roles' => [
            WorkspaceRole::Viewer->value => [
                'permissions' => [
                    // Viewers read everything and run live sessions — see the overview's role
                    // table. What they cannot do is make a change other members would see.
                    PermissionEnum::WorkspaceRead->value,
                    PermissionEnum::SessionRun->value,
                ],
            ],
            WorkspaceRole::Editor->value => [
                'children'    => [WorkspaceRole::Viewer->value],
                'permissions' => [
                    PermissionEnum::WorkspaceWrite->value,
                ],
            ],
            WorkspaceRole::Owner->value => [
                'children'    => [WorkspaceRole::Editor->value],
                'permissions' => [
                    PermissionEnum::WorkspaceManage->value,
                ],
            ],
        ],
    ],
];
