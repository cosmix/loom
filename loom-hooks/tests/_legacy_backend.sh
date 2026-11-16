#!/usr/bin/env bash
# legacy_backend DIR - Select the legacy Codex backend for the guard under test.
# codex.backend defaults to native, where codex-forward-guard.sh refuses every
# forward; tests of the legacy lane must opt in. The guard asks
# `${LOOM_BIN:-loom} hook codex-backend`, so a stub at DIR/loom-legacy that
# answers `legacy` (and exits 0 with no output for anything else) pins the
# backend without reading the operator's real ~/.loom/config.toml or the
# installed binary. Exports LOOM_BIN; call it after the fixture dir exists.
legacy_backend() {
	cat >"$1/loom-legacy" <<'STUB'
#!/usr/bin/env bash
[[ "$1 $2" == "hook codex-backend" ]] && printf '%s\n' legacy
exit 0
STUB
	chmod +x "$1/loom-legacy"
	export LOOM_BIN="$1/loom-legacy"
}
