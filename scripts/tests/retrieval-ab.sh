#!/usr/bin/env bash
set -euo pipefail
repo=$(cd "$(dirname "$0")/../.." && pwd -P)
script="$repo/scripts/retrieval-ab"
tmp=$(mktemp -d "${TMPDIR:-/tmp}/retrieval-ab-test.XXXXXX")
trap 'rm -rf "$tmp"' EXIT
mkdir -p "$tmp/root/retrieval-ab-owned"
printf 'foreign\n' >"$tmp/root/retrieval-ab-owned/data"
if "$script" --clean --out "$tmp/root" 2>"$tmp/error"; then exit 1; fi
rg -q 'refusing' "$tmp/error"
[[ $(<"$tmp/root/retrieval-ab-owned/data") == foreign ]]
ln -s "$tmp/root" "$tmp/link"
if "$script" --clean --out "$tmp/link" 2>"$tmp/error"; then exit 1; fi
rg -q 'refusing' "$tmp/error"
for root in "$repo" "$HOME" "$(dirname "$repo")" /; do
    if "$script" --clean --out "$root" 2>"$tmp/error"; then exit 1; fi
    rg -q 'refusing' "$tmp/error"
done
# A run, not only a clean, refuses foreign content and an unexpected marker.
if "$script" --out "$tmp/root" 2>"$tmp/error"; then exit 1; fi
rg -q 'refusing' "$tmp/error"
mkdir -p "$tmp/wrong/retrieval-ab-owned"
printf 'someone else\n' >"$tmp/wrong/retrieval-ab-owned/.retrieval-ab-owner"
if "$script" --clean --out "$tmp/wrong" 2>"$tmp/error"; then exit 1; fi
rg -q 'refusing' "$tmp/error"
[[ -f "$tmp/wrong/retrieval-ab-owned/.retrieval-ab-owner" ]]
mkdir -p "$tmp/redirect"
ln -s "$tmp/root/retrieval-ab-owned" "$tmp/redirect/retrieval-ab-owned"
if "$script" --clean --out "$tmp/redirect" 2>"$tmp/error"; then exit 1; fi
rg -q 'refusing' "$tmp/error"
[[ -f "$tmp/root/retrieval-ab-owned/data" ]]
mkdir -p "$tmp/owned/retrieval-ab-owned"
printf '%s\n' 'loom retrieval-ab v1' >"$tmp/owned/retrieval-ab-owned/.retrieval-ab-owner"
printf '%s\n' 'keep' >"$tmp/owned/foreign"
"$script" --clean --out "$tmp/owned"
[[ ! -e "$tmp/owned/retrieval-ab-owned" && $(<"$tmp/owned/foreign") == keep ]]
mkdir -p "$tmp/owned/retrieval-ab-owned/measure-root"
printf '%s\n' 'loom retrieval-ab v1' >"$tmp/owned/retrieval-ab-owned/.retrieval-ab-owner"
ln -s "$tmp/root" "$tmp/owned/retrieval-ab-owned/measure-root/baseline"
if "$script" --clean --out "$tmp/owned" 2>"$tmp/error"; then exit 1; fi
rg -q 'refusing' "$tmp/error"
[[ -f "$tmp/root/retrieval-ab-owned/data" ]]
CASES_JSON="$tmp/cases.json" CASE_RESULTS="$tmp/results.ndjson" METRIC1_JSON="$tmp/metric.json"
cat >"$CASES_JSON" <<'JSON'
{"cases":[{"name":"hit","expect":["a"],"forbid":["b"]},{"name":"abstain","abstain":true}]}
JSON
cat >"$CASE_RESULTS" <<'JSON'
{"binary":"baseline","case":"hit","ok":true,"item_ids":["a","b","c","d","e"]}
{"binary":"current","case":"hit","ok":true,"item_ids":["a","b","c","d","e"]}
{"binary":"baseline","case":"abstain","ok":false,"item_ids":[],"error":"legacy failure"}
{"binary":"current","case":"abstain","ok":true,"item_ids":[]}
JSON
source /dev/stdin < <(sed -n '/^score_metric1() {/,/^score_metric23() {/p' "$script" | sed '$d')
score_metric1
jq -e 'all(.aggregates[]; .hit_at_5 == 1 and .raw_precision_at_5 == 0.2 and .forbid_violations == 1) and ([.per_case[] | select(.name == "abstain") | .abstain_expected and .abstention_correct == null] | all) and ([.per_case[] | select(.error == "legacy failure")] | length == 1)' "$METRIC1_JSON" >/dev/null
# Every invocation failing leaves the metrics unavailable, never zero.
cat >"$CASE_RESULTS" <<'JSON'
{"binary":"baseline","case":"hit","ok":false,"item_ids":[],"error":"unavailable"}
{"binary":"current","case":"hit","ok":false,"item_ids":[],"error":"unavailable"}
JSON
score_metric1
jq -e 'all(.aggregates[]; .hit_at_5 == null and .raw_precision_at_5 == null and .mean_mrr == null and .invocation_failures == 2)' "$METRIC1_JSON" >/dev/null
printf '%s\n' 'retrieval-ab tests passed'
