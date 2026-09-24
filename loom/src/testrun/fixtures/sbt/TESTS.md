# sbt fixture scenarios

`example.PassSpec`, `example.FailSpec`, and `example.AlsoPassSpec` each have one
test. The first and third pass; `FailSpec` fails. The single-suite commands
use `sbt 'testOnly example.PassSpec'` or `sbt 'testOnly example.FailSpec'`;
`no-match` selects `example.MissingSpec`. The suite command is `sbt test`.
The build-error scenario models a syntax error before tests run.
