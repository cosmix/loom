// A plain style spec (no CodeMirror import): codemirror.ts wraps it with EditorView.theme.
// Every colour is a design-token variable, so `.dark` on <html> switches it with no second
// palette. CodeMirror's own light and dark base rules still apply underneath (codemirror.ts
// selects them from the page's colour scheme); the rules below override what they colour.
export const themeSpec = {
  "&": {
    color: "var(--foreground)",
    backgroundColor: "var(--background)",
    fontFamily: "var(--font-mono)",
  },
  // The base theme sets `monospace` on the scroller, which wins over the value on `&`.
  ".cm-scroller": {
    fontFamily: "var(--font-mono)",
  },
  // The base theme draws a dotted near-black outline, which disappears on a dark page.
  "&.cm-focused": {
    outline: "1px solid var(--ring)",
  },
  // Without drawSelection the browser draws the caret and the selection itself.
  // The editor edits source text, so `-->` and `=>` must show as typed, not as Inter's arrow
  // ligatures (calt); this covers source and live preview alike.
  ".cm-content": {
    caretColor: "var(--foreground)",
    fontVariantLigatures: "none",
  },
  // --highlight is near-black in the light themes, so the text takes its paired colour.
  ".cm-content ::selection": {
    backgroundColor: "var(--highlight)",
    color: "var(--highlight-foreground)",
  },
  ".cm-gutters": {
    color: "var(--muted-foreground)",
    backgroundColor: "var(--muted)",
    borderRightColor: "var(--border)",
  },
  ".cm-panels": {
    color: "var(--foreground)",
    backgroundColor: "var(--muted)",
    borderColor: "var(--border)",
  },
  ".cm-panels-top": {
    borderBottomColor: "var(--border)",
  },
  ".cm-panels-bottom": {
    borderTopColor: "var(--border)",
  },
  // The search panel's controls: the base theme gives them a light gradient, a white field,
  // 70% type and the monospace face; controls take the body face at 12px.
  ".cm-button, .cm-button:active": {
    backgroundImage: "none",
    backgroundColor: "var(--secondary)",
    color: "var(--secondary-foreground)",
    borderColor: "var(--border)",
    fontSize: "12px",
    fontFamily: "var(--font-sans)",
  },
  ".cm-button:hover, .cm-button:active": {
    backgroundColor: "var(--accent)",
  },
  ".cm-textfield": {
    backgroundColor: "var(--background)",
    color: "var(--foreground)",
    borderColor: "var(--input)",
    fontSize: "12px",
    fontFamily: "var(--font-sans)",
  },
  ".cm-searchMatch": {
    backgroundColor: "var(--primary)",
    color: "var(--primary-foreground)",
  },
  // Validation marks: the dot in the gutter and a faint tint on the gutter cell. The tint is
  // a mix, not a bare variable, so it uses `background` and stays out of the colour check.
  ".cm-mark-gutter .cm-gutterElement": {
    padding: "0 4px",
  },
  ".cm-line-mark": {
    color: "var(--destructive)",
    fontSize: "0.75em",
  },
  ".cm-marked-line": {
    background: "color-mix(in oklch, var(--destructive) 14%, transparent)",
  },
  // Live preview (the `cm-live-preview` class sits on the editor root only in that mode):
  // prose in the body face, headings at the reading view's sizes. Code, tables and inline
  // code keep the monospace face.
  "&.cm-live-preview .cm-content": {
    fontFamily: "var(--font-sans)",
    lineHeight: "1.65",
  },
  ".cm-lp-h1, .cm-lp-h2, .cm-lp-h3, .cm-lp-h4, .cm-lp-h5, .cm-lp-h6": {
    fontFamily: "var(--font-sans)",
    fontWeight: "600",
    lineHeight: "1.3",
    paddingTop: "0.35em",
  },
  ".cm-lp-h1": { fontSize: "1.5em", fontWeight: "650", letterSpacing: "-0.02em" },
  ".cm-lp-h2": { fontSize: "1.25em", fontWeight: "650" },
  ".cm-lp-h3": { fontSize: "1.08em" },
  ".cm-lp-h4": { fontSize: "1em" },
  ".cm-lp-h5, .cm-lp-h6": { fontSize: "0.9em", color: "var(--muted-foreground)" },
  // Fenced code sits on the muted ground; the first and last lines round the block.
  ".cm-lp-code": {
    position: "relative",
    fontFamily: "var(--font-mono)",
    fontSize: "0.92em",
    lineHeight: "1.5",
    backgroundColor: "var(--muted)",
    paddingLeft: "0.75rem",
    paddingRight: "0.75rem",
  },
  ".cm-lp-code-first": {
    borderTopLeftRadius: "6px",
    borderTopRightRadius: "6px",
    paddingTop: "0.4em",
  },
  ".cm-lp-code-last": {
    borderBottomLeftRadius: "6px",
    borderBottomRightRadius: "6px",
    paddingBottom: "0.4em",
  },
  ".cm-lp-fence-label": {
    position: "absolute",
    top: "0.35em",
    right: "0.75rem",
    fontFamily: "var(--font-mono)",
    fontSize: "0.7em",
    letterSpacing: "0.08em",
    textTransform: "uppercase",
    color: "var(--muted-foreground)",
    userSelect: "none",
    pointerEvents: "none",
  },
  ".cm-lp-mono": { fontFamily: "var(--font-mono)", fontSize: "0.92em" },
  ".cm-lp-dim": { color: "var(--muted-foreground)" },
  ".cm-lp-inline-code": {
    fontFamily: "var(--font-mono)",
    fontSize: "0.92em",
    backgroundColor: "var(--muted)",
    borderRadius: "3px",
    padding: "0.1em 0.25em",
  },
  ".cm-lp-bullet": {
    display: "inline-block",
    width: "1em",
    textAlign: "center",
    fontWeight: "700",
    color: "var(--muted-foreground)",
  },
  ".cm-lp-quote-bar": {
    display: "inline-block",
    height: "1.2em",
    verticalAlign: "text-bottom",
    borderLeftWidth: "3px",
    borderLeftStyle: "solid",
    borderLeftColor: "var(--border)",
  },
  ".cm-lp-check": {
    margin: "0 0.35em 0 0",
    verticalAlign: "middle",
    cursor: "pointer",
    accentColor: "var(--primary)",
  },
  ".cm-lp-done": { color: "var(--muted-foreground)" },
};
