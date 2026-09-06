<?php

declare(strict_types=1);

namespace App\Storage;

/**
 * Object keys are content-addressed: `sheets/{workspace}/{sha256}.pdf`.
 *
 * Two consequences fall out of that, both of which the sheet-attachments spec wanted anyway.
 * Re-uploading identical bytes writes the same key, so "a replace producing the same hash is a
 * no-op" stops being a rule the code has to remember. And replacing a file writes a *new*
 * object rather than mutating an existing one, so a device still holding the old URL keeps
 * receiving the bytes it cached instead of silently getting different ones.
 *
 * The workspace id stays in the key even though the content database no longer has that column:
 * it comes from the route, and it keeps a workspace's objects deletable as a single prefix.
 */
final class SheetKey
{
    public static function for(string $workspaceId, string $sha256): string
    {
        return sprintf('sheets/%s/%s.pdf', $workspaceId, strtolower($sha256));
    }

    public static function prefixFor(string $workspaceId): string
    {
        return sprintf('sheets/%s/', $workspaceId);
    }

    public static function isValidSha256(string $value): bool
    {
        return preg_match('/^[a-f0-9]{64}$/i', $value) === 1;
    }
}
