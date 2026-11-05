# M1: the screenshot scripts, the browser theme check and the docs take the new theme ids (R9)

Tier: haiku (`loom-mechanical-engineer`).

Read `../common.md` first. The planner's themes become Ledger (light), Walnut (dark, the default
dark theme) and Graphite (dark). Aubergine and Pacific are removed. The stored dark-variant ids are
`walnut` and `gray` (Graphite keeps `gray`); V1 changes the tokens and the theme picker beside
you. Your files only take the new ids and names. After you finish, none of your files may contain
the words `aubergine` or `pacific` in any case (an after_stage check greps for them).

## Edits

1. `web/scripts/plan-screens.mjs`
   - Lines 5-6 (header comment): "in Ledger (light) and Walnut (dark) unless --themes names
     others from THEMES".
   - `THEMES` (lines 22-27) becomes exactly:
     ```js
     const THEMES = {
       ledger: { name: "ledger", scheme: "light", dark: "walnut" },
       walnut: { name: "walnut", scheme: "dark", dark: "walnut" },
       graphite: { name: "graphite", scheme: "dark", dark: "gray" },
     };
     ```
   - `DEFAULT_THEMES` (line 28) becomes `["ledger", "walnut"]`.
2. `scripts/plan-screens.sh` lines 9-11 (header comment): "in Ledger and Walnut, or the --themes
   ids (ledger, walnut, graphite)". Nothing else changes.
3. `web/scripts/theme-preview-test.mjs` lines 17-22: the list becomes
   `["light", "ledger"], ["dark", "walnut"], ["dark", "gray"]`; line 32's fallback
   `"aubergine"` becomes `"walnut"`.
4. `web/src/components/settings-webui.tsx` line 11: in the search keyword string replace
   `aubergine pacific` with `walnut`, so it reads
   `... light dark blue gray grey ledger walnut graphite notifications ...`. Keep every other word
   (`settings-webui.test.tsx:190` filters on `graphite`).
5. `README.md` line 742: "a theme picker (Ledger, Walnut, Graphite, each drawn as a miniature of the
   stage graph in that theme's own colors)". Nothing else in the README changes.

## Do not

- Edit any other file, any test, or `web/src/components/theme-picker.tsx` (V1's).
- Run the browser scripts (no browser in your session).

## Run once at the end

From the repository root: `rg -n -i 'aubergine|pacific' web/scripts scripts README.md web/src/components/settings-webui.tsx`
It must print nothing.

Report: files changed and the command's output.
