# Mix test fixture cases

The modeled `test/alpha_test.exs` suite has three ExUnit tests: `test alpha passes`,
`test alpha fails`, and `test beta passes`. The first and third pass; the second
fails. `mix test` runs all three. The selected cases use `mix test
test/alpha_test.exs --only 'test:<name>'` with the complete ExUnit test name.

`no-match` and `no-match.excluded` select `test missing`; they cover two
documented-format summary forms for no executed test. `build-error` models a
compile error in `test/broken_test.exs` before ExUnit finishes.
