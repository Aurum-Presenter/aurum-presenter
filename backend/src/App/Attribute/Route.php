<?php

declare(strict_types=1);

namespace App\Attribute;

use Attribute;

#[Attribute(Attribute::TARGET_METHOD | Attribute::TARGET_CLASS | Attribute::IS_REPEATABLE)]
final readonly class Route
{
    /**
     * @param string[] $methods
     * @param array<string, mixed> $options
     */
    public function __construct(
        public string $path,
        public array $methods = ['GET'],
        public ?string $name = null,
        public array $options = [],
    ) {
    }
}
