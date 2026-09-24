# Maven fixture scenarios

The documented-format suite has `example.AlphaTest#passes`,
`example.AlphaTest#fails`, and `example.BetaTest#passes`. The first and third
pass; the second fails. These names describe synthetic fixture output, not a
project captured on the plan author host.

The one-test filters use `mvn test -Dtest=AlphaTest#passes` and
`mvn test -Dtest=AlphaTest#fails`. The no-match filter is
`mvn test -Dtest=MissingTest#absent`. The suite uses `mvn test`.
The build-error scenario also uses `mvn test`, with a syntax error in a test
source file before Surefire starts.
