# Brief Footer Stage Flag

> Topic notes for the concerns knowledge area.

**Live, in every stage signal, and reproducible in one command.**

`orchestrator/signals/format/brief.rs:46` renders the Knowledge Brief footer as:

```text
Pull more with:

    loom knowledge context --stage {} --query "<question>" --budget-tokens <n>
```

But `loom knowledge context` accepts no `--stage`. Running exactly what the brief tells the
agent to run yields:

```text
error: unexpected argument '--stage' found
Usage: loom knowledge context [OPTIONS] --query <QUERY>
```

The real flags are `--query` (required), `--budget-tokens` (default 2000), `--scope`
(`knowledge`|`source`|`all`, default `all`), `--require-id` (repeatable), `--explain`, `--json`.
Dropping `--stage` makes the command work; retrieval is not stage-parameterised on the CLI at
all.

So the one call-to-action embedded in every Knowledge Brief fails, and it fails only when an
agent actually tries it — which is why four implementation stages and a full verification gate
did not surface it.

## Why the test suite could not catch it

`brief.rs:200` asserts the rendered footer equals that exact string. **An equality test on
advertised command text pins whatever was written, correct or not**; it can never notice the
flag does not exist. This is the `mistakes/tests-that-cannot-fail.md` shape in a new place.

## Fix

Drop `--stage {}` from `brief.rs:46` (and the interpolated stage id with it), and change the
test at `:200` to PARSE the advertised string with the real clap command rather than compare it
to a literal.

**Generalisable rule: when generated output tells a human or an agent to run a command, the
test must execute or parse that command with the real argument parser, never string-match it.**

Not fixed at the distillation gate that found it because `loom/src/**` is outside that stage's
declared write scope.
