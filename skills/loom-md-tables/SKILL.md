---
name: loom-md-tables
description: Fix markdown table alignment and spacing issues. Use when formatting tables in markdown files, aligning columns, normalizing cell padding, or ensuring proper GFM table structure. Runs a Python script to normalize column widths while preserving alignment markers.
allowed-tools:
  - Read
  - Edit
  - Bash(python *)
triggers:
  - markdown table
  - md table
  - table formatting
  - column alignment
  - pipe table
  - GFM table
  - table generator
  - align columns
  - table spacing
  - table layout
  - fix table
  - format table
  - table structure
---

# Markdown Table Formatting

Utility for fixing markdown table alignment and spacing. Normalizes column widths, ensures consistent padding, and preserves alignment markers.

## Quick Examples

```bash
# Preview fixed output
python fix-md-tables.py document.md

# Fix in-place
python fix-md-tables.py document.md -i
```

### Common Patterns

**Status tables:**

```markdown
| Stage | Status    | Branch     |
| ----- | --------- | ---------- |
| build | Complete  | loom/build |
| test  | Executing | loom/test  |
```

**Configuration tables:**

```markdown
| Option     | Default | Description           |
| ---------- | ------- | --------------------- |
| timeout    | 300     | Session timeout (sec) |
| auto_merge | false   | Enable auto merging   |
```

**Right-aligned numbers** (script preserves the `---:` marker but left-aligns and right-pads the cell *text* — GFM still renders these right-aligned when displayed):

```markdown
| Item  | Count | Total |
| ----- | ----: | ----: |
| Files | 42    | 500   |
| Lines | 1,234 | 5,000 |
```

## Features

- Aligns each column to its widest cell (separators get a minimum width of 3)
- Single-space padding; cell **content is always left-aligned and right-padded** (`ljust`) regardless of the alignment marker
- Preserves the alignment marker in the separator row (`:---`, `:---:`, `---:`), so GFM rendering still honors it
- Uses the header's column count: short body rows gain empty trailing cells and excess body cells are dropped, matching GFM rendering
- Preserves a shared zero-to-three-space Markdown indent and recognizes GFM one-cell body rows without a pipe
- Inserts a blank line before and after each table when missing

## Invocation

The script lives in this skill's directory, so pass its path (or copy it to the target dir). `allowed-tools` permits `Bash(python *)`; invoke via `python`, not `./`.

```bash
python /path/to/skills/loom-md-tables/fix-md-tables.py FILE.md      # preview to stdout
python /path/to/skills/loom-md-tables/fix-md-tables.py FILE.md -i   # -i or --in-place: rewrite
```

⚠ A pipe-separated header must be immediately followed by a valid GFM delimiter row with the same number of cells and indent. The utility preserves escaped `\|` inside cells and ignores fenced code, raw HTML blocks, four-space indented code, and lines starting another block such as a blockquote or list item. Pipe-looking prose without a delimiter row is left unchanged. Leading/trailing pipes are optional under GFM; the normalized output uses them for readability. Preview the diff when a table sits inside a complex nested container.

## Alignment Markers

| Syntax  | Alignment      |
| ------- | -------------- |
| `---`   | Left (default) |
| `:---`  | Left           |
| `---:`  | Right          |
| `:---:` | Center         |

## Verify before done

- [ ] Ran preview (no `-i`) first and eyeballed the diff before rewriting in place.
- [ ] Separator markers (`:---`, `---:`, `:---:`) survived; alignment intent intact.
- [ ] Header and delimiter counts/indents match; pipe-looking prose plus fenced/raw-HTML examples were intentionally left unchanged.
