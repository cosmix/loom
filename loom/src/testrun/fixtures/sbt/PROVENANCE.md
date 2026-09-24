Written from the runner documented output format; the runner was not installed on the plan author host. Replace with captured output when available.

- Exit 0: `one-pass`, `no-match`. Uncertain until captured on an sbt host.
- Exit 1: `one-fail`, `suite`, `build-error`. Uncertain until captured on an sbt host.

The exit values follow the stage's sbt contract; the console text and codes are not host captures.
