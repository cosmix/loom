# SwiftPM XCTest fixture scenarios

The imagined `ModuleTests` package has `FooTests.testBar` and `FooTests.testBaz` passing, and `FooTests.testFails` failing. `one-pass` runs `swift test --filter 'ModuleTests.FooTests/testBar'`; `one-fail` selects `testFails`; `no-match` selects `testMissing`; `suite` runs `swift test`. `build-error` represents a compiler diagnostic before XCTest starts.
