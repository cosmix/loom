# aurora-ui (vendored)

Copied from the aurora-ui kit (`shared/`, `feedback/`, `theme/`): the busy roundel,
hazard panels, empty state, error boundary, theme atoms and toggle, and the OKLCH
token sheet. Files are kept as copied and excluded from the formatter and linter;
the one local change is `shared/ui/button.tsx`, which re-exports this app's shadcn
Button so the kit needs no second primitive library, and the storage prefix in
`shared/atoms/theme.ts`.
