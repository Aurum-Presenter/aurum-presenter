<?php

declare(strict_types=1);

namespace App\Auth;

use App\Enum\PermissionEnum;
use App\Enum\WorkspaceRole;
use Laminas\Permissions\Rbac\Rbac;
use Laminas\Permissions\Rbac\Role;

/**
 * The single enforcement point that replaced Postgres row-level security.
 *
 * The role graph is built once from config. Note the inversion that the stack's authorization
 * reference warns about: the config's `children` key names the roles a role ABSORBS, because
 * Laminas' Role::hasPermission() recurses into children — permissions bubble *upward*. So
 * `owner => children: [editor]` means an owner holds everything an editor holds.
 *
 * There are no entity assertions here, and that is a direct consequence of the per-workspace
 * database file: an assertion would exist to answer "is this record in a workspace the caller
 * belongs to", and that question is already answered by which file got opened.
 */
final class AuthorizationService
{
    private readonly Rbac $rbac;

    /** @param array<string, array{permissions?: string[], children?: string[]}> $roleConfig */
    public function __construct(array $roleConfig)
    {
        $this->rbac = new Rbac();
        $this->rbac->setCreateMissingRoles(false);

        foreach (array_keys($roleConfig) as $roleName) {
            if (! $this->rbac->hasRole($roleName)) {
                $this->rbac->addRole(new Role($roleName));
            }
        }

        foreach ($roleConfig as $roleName => $definition) {
            $role = $this->rbac->getRole($roleName);

            foreach ($definition['permissions'] ?? [] as $permission) {
                $role->addPermission($permission);
            }

            foreach ($definition['children'] ?? [] as $childName) {
                if (! $this->rbac->hasRole($childName)) {
                    $this->rbac->addRole(new Role($childName));
                }

                $role->addChild($this->rbac->getRole($childName));
            }
        }
    }

    public function isGranted(WorkspaceRole $role, PermissionEnum $permission): bool
    {
        if (! $this->rbac->hasRole($role->value)) {
            return false;
        }

        return $this->rbac->isGranted($role->value, $permission->value);
    }
}
