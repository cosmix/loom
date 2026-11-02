<?php

describe('suite', function () {
    it('value (a)+', function () {
        if (getenv('VER01_SKIP_EXACT')) {
            $this->markTestSkipped('exact target intentionally skipped');
        }
        expect(true)->toBeTrue();
    });

    it('value (a)+ suffix', fn () => throw new Exception('suffix sibling ran'));
    it('prefix value (a)+', fn () => throw new Exception('prefix sibling ran'));
});
