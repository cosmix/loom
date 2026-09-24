# PHPUnit fixture scenarios

The documented output represents `tests/ExampleTest.php` with three methods:
`testAlphaPasses`, `testBetaFails`, and `testGammaPasses`.

- `one-pass`: `vendor/bin/phpunit --filter 'testAlphaPasses' tests/ExampleTest.php`
- `one-fail`: `vendor/bin/phpunit --filter 'testBetaFails' tests/ExampleTest.php`
- `no-match`: `vendor/bin/phpunit --filter 'testMissing' tests/ExampleTest.php`
- `suite`: `vendor/bin/phpunit`
