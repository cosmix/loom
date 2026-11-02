<?php

use PHPUnit\Framework\Attributes\DataProvider;
use PHPUnit\Framework\TestCase;

final class CollisionTest extends TestCase
{
    public static function cases(): array
    {
        return [
            'value (a)+' => ['exact'],
            'value (a)+ suffix' => ['suffix'],
            'prefix value (a)+' => ['prefix'],
        ];
    }

    #[DataProvider('cases')]
    public function testValue(string $case): void
    {
        if ($case === 'exact') {
            if (getenv('VER01_SKIP_EXACT')) {
                self::markTestSkipped('exact target intentionally skipped');
            }
            self::assertTrue(true);
            return;
        }
        self::fail("{$case} sibling ran");
    }
}
