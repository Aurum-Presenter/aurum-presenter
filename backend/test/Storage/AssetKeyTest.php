<?php

declare(strict_types=1);

namespace AppTest\Storage;

use App\Http\ApiException;
use App\Storage\AssetKey;
use PHPUnit\Framework\Attributes\DataProvider;
use PHPUnit\Framework\TestCase;

/**
 * The key an audience background is stored under.
 *
 * Two things matter here. The key is derived from the bytes, so the same image uploaded from
 * two laptops is one object and a device that cached it under that name never has to be told
 * it changed. And the type decides the extension, so nothing a browser would execute can be
 * given a key at all.
 */
final class AssetKeyTest extends TestCase
{
    public function testTheKeyIsTheWorkspaceTheHashAndTheType(): void
    {
        self::assertSame(
            'assets/ws-1/' . str_repeat('a', 64) . '.png',
            AssetKey::for('ws-1', str_repeat('A', 64), 'image/png'),
        );
    }

    public function testTheSameBytesGiveTheSameKey(): void
    {
        $hash = hash('sha256', 'the same picture');

        self::assertSame(
            AssetKey::for('ws-1', $hash, 'image/jpeg'),
            AssetKey::for('ws-1', $hash, 'image/jpeg'),
        );
    }

    public function testOneWorkspacesAssetsAreNotAnothers(): void
    {
        $hash = hash('sha256', 'the same picture');

        self::assertNotSame(AssetKey::for('ws-1', $hash, 'image/png'), AssetKey::for('ws-2', $hash, 'image/png'));
    }

    /** @return list<array{string, string}> */
    public static function types(): array
    {
        return [
            ['image/png', 'png'],
            ['image/jpeg', 'jpg'],
            ['image/webp', 'webp'],
            ['image/avif', 'avif'],
            ['IMAGE/PNG', 'png'],
        ];
    }

    #[DataProvider('types')]
    public function testEveryDisplayableTypeHasAnExtension(string $contentType, string $extension): void
    {
        self::assertSame($extension, AssetKey::extensionFor($contentType));
    }

    public function testAnythingElseIsRefused(): void
    {
        $this->expectException(ApiException::class);
        $this->expectExceptionMessage('A background must be a PNG, JPEG, WebP or AVIF image.');

        AssetKey::extensionFor('image/svg+xml');
    }

    /** The reader has to try each type to find an object, so the list must not be empty. */
    public function testTheDisplayableTypesAreListed(): void
    {
        self::assertContains('image/png', AssetKey::contentTypes());
        self::assertNotContains('image/svg+xml', AssetKey::contentTypes());
    }
}
