#!/usr/bin/env python3
"""Fix markdown table alignment and spacing issues."""

import sys
from pathlib import Path


def is_table_row(line: str) -> bool:
    """Check if a line is a markdown table row."""
    stripped = line.strip()
    return stripped.startswith("|") and stripped.endswith("|")


def is_separator_row(line: str) -> bool:
    """Check if a line is a table separator row (|---|---|)."""
    stripped = line.strip()
    if not is_table_row(line):
        return False
    # Remove pipes and check if only dashes, colons, and spaces remain
    content = stripped[1:-1]  # Remove outer pipes
    return all(c in "-|: " for c in content)


def normalize_table_row(line: str) -> str:
    """Normalize spacing in a table row."""
    if not is_table_row(line):
        return line

    # Split by pipe, preserving structure
    stripped = line.strip()
    cells = stripped.split("|")

    # First and last are empty (before first | and after last |)
    # Middle cells are the actual content
    normalized_cells = []
    for i, cell in enumerate(cells):
        if i == 0 or i == len(cells) - 1:
            # Keep empty strings for outer pipes
            normalized_cells.append("")
        else:
            # Normalize cell content - single space padding
            content = cell.strip()
            normalized_cells.append(f" {content} ")

    return "|".join(normalized_cells)


def align_table(table_lines: list[str]) -> list[str]:
    """Align a markdown table by normalizing column widths."""
    if not table_lines:
        return table_lines

    # Parse all rows into cells
    rows = []
    for line in table_lines:
        stripped = line.strip()
        cells = stripped.split("|")[1:-1]  # Remove outer empty strings
        rows.append([cell.strip() for cell in cells])

    if not rows:
        return table_lines

    # Find the number of columns
    num_cols = max(len(row) for row in rows)

    # Pad rows with fewer columns
    for row in rows:
        while len(row) < num_cols:
            row.append("")

    # Calculate max width for each column
    col_widths = []
    for col in range(num_cols):
        max_width = 0
        for row in rows:
            if col < len(row):
                # For separator rows, use minimum width of 3
                cell = row[col]
                if all(c in "-:" for c in cell):
                    max_width = max(max_width, 3)
                else:
                    max_width = max(max_width, len(cell))
        col_widths.append(max_width)

    # Rebuild the table with aligned columns
    aligned_lines = []
    for row in rows:
        cells = []
        for col_idx, cell in enumerate(row):
            width = col_widths[col_idx]
            if all(c in "-:" for c in cell):
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
        aligned_lines.append("| " + " | ".join(cells) + " |")

    return aligned_lines


def fix_tables_in_content(content: str) -> str:
    """Fix all tables in markdown content."""
    lines = content.split("\n")
    result = []
    i = 0

    while i < len(lines):
        line = lines[i]

        # Check if this is the start of a table
        if is_table_row(line):
            # Collect all table lines
            table_lines = []
            while i < len(lines) and is_table_row(lines[i]):
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
