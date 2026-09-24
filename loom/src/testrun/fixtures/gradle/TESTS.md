# Gradle fixture tests

The documented-format project has `sample.AlphaTest.passes`, `sample.AlphaTest.fails`, and `sample.AlphaTest.alsoPasses`. `one-pass` and `one-fail` use `./gradlew test --tests '<method>'`; `no-match` selects a missing method. `suite` and `build-error` use `./gradlew test`.
