# S1: plan summaries cover the brief bundle (bundle-summaries)

Read `../common.md` first, then the stage description's DESIGN paragraph, which is your
specification. You are the only worker; the contracts in
`loom/src/planning/summary/seal/bundle_contract_tests.rs` are frozen when you start.

## Read before writing

The merged two-level plan summary (PLAN-planner-follow-ups, summary-fragments):
`seal/links.rs` (`Rules::allowed_sources`, `source_problem`, `uncited`), the levels check and
`seal/levels_contract_tests.rs`, `format/front.rs` (`RawFront`, `FrontMatter`, `parse_front`,
`render_front`, all hand-written), `seal.rs` (where `file` and `cites` are computed),
`evaluate.rs` (`evaluate_one`), `evaluate/cite.rs` (`Kind::Document`, `holds`, `complete`),
`store.rs` (`summary_id_for`, `source_of`), `brief.rs` (`sources`, `write_rules`), and
`planning/briefs.rs` (`bundle_files`, added by trace-core; common.md "The bundle"). Check how `LockedSet` treats brief files
(`planning/paths.rs` lock reasons): `content_hash` must return a hash for a brief of a locked
plan, or the bundle of a running plan cannot be checked; read before relying on it.

## Tasks

1. Use `briefs::bundle_files(project, plan_rel)` for every bundle lookup (seal, evaluation, rebuild
   brief); add no second bundle helper.
2. **Seal.** Allowed sources for a plan id: the plan plus its bundle. The "every brief cited"
   rule for plan ids (refusal text in the stage description). Compute `bundle` front matter at
   seal time from each brief's content hash; refuse a draft that carries its own `bundle` (front
   matter is computed, never taken from the draft, as for `file`).
3. **Front matter.** Add `bundle: BTreeMap<String, String>` (rendered only when non-empty;
   rejected on overviews and on non-plan document summaries).
4. **Evaluation.** `Kind::Document` carries the bundle; `holds` accepts a Source link to a bundle
   file with the same span-hash check as the plan; `complete` also compares the bundle map with
   the current bundle.
5. **Rebuild brief and prompts.** `brief.rs::sources` lists the bundle; the rules line for plan
   ids says the summary may cite the plan and its briefs and must cite every brief file.
   `plan-plugin/agents/summary.md` and `guidance/text/summary_codex.md`: one detail paragraph per
   stage citing `PLAN-<x>.md#stage:<id>` and that stage's brief files; `common.md` and other bundle
   files cited where the plan relies on them; read every brief named under Sources.
6. **Spec.** `doc/product/feat-planner/section_04.md`: the bundle rule, the front-matter field and
   when a plan summary goes out of date. Present truth only.

## Tests

Unit tests beside each change (front matter round trip with and without `bundle`, rejection on an
overview, evaluation with a removed brief, `holds` for a brief anchor). Make the five frozen
contracts pass without editing them.

## Done

The stage acceptance commands pass; no file under `summary/` over 400 lines (`store.rs` 352 and
`format/body.rs` 350 before you start); report any merged name that differs from this brief.
