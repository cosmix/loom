#!/usr/bin/env python3

"""Fix markdown table alignment and spacing issues."""

import re
import sys
from pathlib import Path


FENCE_RE = re.compile(r"^ {0,3}(`{3,}|~{3,})(.*)$")
SEPARATOR_CELL_RE = re.compile(r"^:?-+:?$")
ATX_HEADING_RE = re.compile(r"^#{1,6}(?:[ \t]+|$)")
LIST_MARKER_RE = re.compile(r"^(?:[-+*]|\d{1,9}[.)])(?:[ \t]+|$)")
LINK_REFERENCE_RE = re.compile(r"^\[[^]]+\]:")
THEMATIC_BREAK_RE = re.compile(
    r"^(?:(?:\*[ \t]*){3,}|(?:-[ \t]*){3,}|(?:_[ \t]*){3,})$"
)
RAW_HTML_TAG_RE = re.compile(r"^<(script|pre|style|textarea)(?:[ \t]|>|$)", re.I)
RAW_HTML_BLOCK_TAG_RE = re.compile(
    r"^</?(?:address|article|aside|base|basefont|blockquote|body|caption|center|"
    r"col|colgroup|dd|details|dialog|dir|div|dl|dt|fieldset|figcaption|figure|"
    r"footer|form|frame|frameset|h[1-6]|head|header|hr|html|iframe|legend|li|"
    r"link|main|menu|menuitem|nav|noframes|ol|optgroup|option|p|param|search|"
    r"section|summary|table|tbody|td|tfoot|th|thead|title|tr|track|ul)"
    r"(?:[ \t]|/?>|$)",
    re.I,
)
RAW_HTML_COMPLETE_TAG_RE = re.compile(
    r"^</?[A-Za-z][A-Za-z0-9-]*(?:[ \t]+[^<>]*)?/?>[ \t]*$"
)


def markdown_indent(line: str) -> int | None:
    """Return a valid zero-to-three-space Markdown indent, else None."""
    spaces = len(line) - len(line.lstrip(" "))
    if spaces > 3 or line[spaces:].startswith("\t"):
        return None
    return spaces


def markdown_content(line: str) -> str | None:
    """Return content after an optional top-level GFM indent."""
    indent = markdown_indent(line)
    return None if indent is None else line[indent:]


def has_unescaped_pipe(text: str) -> bool:
    """Return whether text contains a pipe that acts as a GFM delimiter."""
    preceding_backslashes = 0
    for char in text:
        if char == "|" and preceding_backslashes % 2 == 0:
            return True
        if char == "\\":
            preceding_backslashes += 1
        else:
            preceding_backslashes = 0
    return False


def raw_html_block_start(line: str) -> tuple[str, re.Pattern[str] | None] | None:
    """Identify the common GFM raw-HTML block forms and their terminator."""
    content = markdown_content(line)
    if content is None:
        return None

    tag_match = RAW_HTML_TAG_RE.match(content)
    if tag_match:
        tag = re.escape(tag_match.group(1))
        return "marker", re.compile(rf"</{tag}[ \t]*>", re.I)
    if content.startswith("<!--"):
        return "marker", re.compile(r"-->")
    if content.startswith("<?"):
        return "marker", re.compile(r"\?>")
    if content.startswith("<![CDATA["):
        return "marker", re.compile(r"\]\]>")
    if re.match(r"^<![A-Z]", content):
        return "marker", re.compile(r">")
    if RAW_HTML_BLOCK_TAG_RE.match(content):
        return "blank", None
    if RAW_HTML_COMPLETE_TAG_RE.fullmatch(content):
        return "blank", None
    return None


def starts_block(line: str) -> bool:
    """Return whether a line starts a block that terminates a GFM table."""
    content = markdown_content(line)
    if content is None or not content.strip():
        return True
    return bool(
        FENCE_RE.match(line)
        or content.startswith(">")
        or ATX_HEADING_RE.match(content)
        or LIST_MARKER_RE.match(content)
        or LINK_REFERENCE_RE.match(content)
        or THEMATIC_BREAK_RE.fullmatch(content.strip())
        or raw_html_block_start(line)
    )


def split_table_row(line: str) -> list[str]:
    """Split a GFM table row without treating an escaped pipe as a delimiter."""
    stripped = line.strip()
    cells: list[str] = []
    current: list[str] = []
    preceding_backslashes = 0

    for char in stripped:
        if char == "|" and preceding_backslashes % 2 == 0:
            cells.append("".join(current).strip())
            current = []
            preceding_backslashes = 0
            continue
        current.append(char)
        if char == "\\":
            preceding_backslashes += 1
        else:
            preceding_backslashes = 0
    cells.append("".join(current).strip())

    if stripped.startswith("|"):
        cells.pop(0)
    trailing_backslashes = len(stripped[:-1]) - len(stripped[:-1].rstrip("\\"))
    if stripped.endswith("|") and trailing_backslashes % 2 == 0:
        cells.pop()
    return cells


def is_table_row(line: str) -> bool:
    """Check whether a line contains a GFM-style, pipe-separated row."""
    content = markdown_content(line)
    return bool(
        content is not None
        and not starts_block(line)
        and has_unescaped_pipe(content)
        and split_table_row(line)
    )


def is_separator_cell(cell: str) -> bool:
    """Check whether a cell is a valid GFM table delimiter cell."""
    return bool(SEPARATOR_CELL_RE.fullmatch(cell.strip()))


def is_separator_row(line: str) -> bool:
    """Check whether every cell is a valid GFM table delimiter cell."""
    return is_table_row(line) and all(
        is_separator_cell(cell) for cell in split_table_row(line)
    )


def align_table(table_lines: list[str]) -> list[str]:
    """Align a markdown table by normalizing column widths."""
    if not table_lines:
        return table_lines

    # Parse all rows into cells
    rows = []
    indents = []
    for line in table_lines:
        rows.append(split_table_row(line))
        indent = markdown_indent(line)
        indents.append(" " * (indent or 0))

    if not rows:
        return table_lines

    # GFM fixes the table width from its header. Body rows may be ragged, but
    # additional cells are ignored and missing cells are empty.
    num_cols = len(rows[0])

    # Trim extra body cells and pad missing cells to the header's width.
    for row in rows:
        del row[num_cols:]
        while len(row) < num_cols:
            row.append("")

    # Calculate max width for each column
    col_widths = []
    for col in range(num_cols):
        max_width = 0
        for row_idx, row in enumerate(rows):
            if col < len(row):
                # For separator rows, use minimum width of 3
                cell = row[col]
                if row_idx == 1:
                    max_width = max(max_width, 3)
                else:
                    max_width = max(max_width, len(cell))
        col_widths.append(max_width)

    # Rebuild the table with aligned columns
    aligned_lines = []
    for row_idx, row in enumerate(rows):
        cells = []
        for col_idx, cell in enumerate(row):
            width = col_widths[col_idx]
            if row_idx == 1:
                # Separator row - preserve alignment markers
                if cell.startswith(":") and cell.endswith(":"):
                    cells.append(":" + "-" * (width - 2) + ":")
                elif cell.startswith(":"):
                    cells.append(":" + "-" * (width - 1))
                elif cell.endswith(":"):
                    cells.append("-" * (width - 1) + ":")
                else:
                    cells.append("-" * width)
            else:
                # Regular cell - left-align with padding
                cells.append(cell.ljust(width))
        aligned_lines.append(indents[row_idx] + "| " + " | ".join(cells) + " |")

    return aligned_lines


def fix_tables_in_content(content: str) -> str:
    """Fix all tables in markdown content."""
    lines = content.split("\n")
    result = []
    i = 0
    fence: tuple[str, int] | None = None
    html_block: tuple[str, re.Pattern[str] | None] | None = None

    while i < len(lines):
        line = lines[i]
        fence_match = FENCE_RE.match(line)

        if fence is not None:
            if fence_match:
                marker = fence_match.group(1)
                if (
                    marker[0] == fence[0]
                    and len(marker) >= fence[1]
                    and not fence_match.group(2).strip()
                ):
                    fence = None
            result.append(line)
            i += 1
            continue

        if html_block is not None:
            result.append(line)
            kind, end_pattern = html_block
            if kind == "blank" and not line.strip():
                html_block = None
            elif kind == "marker" and end_pattern and end_pattern.search(line):
                html_block = None
            i += 1
            continue

        if fence_match:
            marker = fence_match.group(1)
            fence = (marker[0], len(marker))
            result.append(line)
            i += 1
            continue

        html_start = raw_html_block_start(line)
        if html_start is not None:
            kind, end_pattern = html_start
            result.append(line)
            if not (kind == "marker" and end_pattern and end_pattern.search(line)):
                html_block = html_start
            i += 1
            continue

        # GFM recognizes a table only with a header followed immediately by a
        # matching delimiter row. This avoids rewriting ASCII/prose pipe lines.
        if (
            is_table_row(line)
            and i + 1 < len(lines)
            and is_separator_row(lines[i + 1])
            and markdown_indent(line) == markdown_indent(lines[i + 1])
            and len(split_table_row(line)) == len(split_table_row(lines[i + 1]))
        ):
            table_indent = markdown_indent(line)
            table_lines = [line, lines[i + 1]]
            i += 2
            # Once a table is established, GFM treats every nonblank line as a
            # body row until another block starts. That includes a one-cell row
            # with no pipe. Requiring the header's indent avoids crossing out of
            # an indented container while normalizing it.
            while (
                i < len(lines)
                and markdown_indent(lines[i]) == table_indent
                and not starts_block(lines[i])
            ):
                table_lines.append(lines[i])
                i += 1

            # Check if we need a blank line before the table
            if result and result[-1].strip() != "":
                result.append("")

            # Align and add the table
            aligned = align_table(table_lines)
            result.extend(aligned)

            # Check if we need a blank line after the table
            if i < len(lines) and lines[i].strip() != "":
                result.append("")
        else:
            result.append(line)
            i += 1

    return "\n".join(result)


def main():
    if len(sys.argv) < 2:
        print("Usage: python fix-md-tables.py <file.md> [--in-place]")
        sys.exit(1)

    filepath = Path(sys.argv[1])
    in_place = "--in-place" in sys.argv or "-i" in sys.argv

    if not filepath.exists():
        print(f"Error: {filepath} does not exist")
        sys.exit(1)

    content = filepath.read_text()
    fixed = fix_tables_in_content(content)

    if in_place:
        filepath.write_text(fixed)
        print(f"Fixed tables in {filepath}")
    else:
        print(fixed)


if __name__ == "__main__":
    main()
