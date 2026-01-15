#!/usr/bin/env bash
# skill-index-builder.sh - Build keyword index from SKILL.md files
#
# Parses YAML frontmatter from all SKILL.md files in ~/.claude/skills/
# and builds an inverted index: keyword -> [skill names]
#
# Output: ~/.claude/hooks/loom/skill-keywords.json
#
# Supports two trigger formats in SKILL.md frontmatter:
#   1. YAML list format:
#      triggers:
#        - keyword1
#        - keyword2
#
#   2. Comma-separated format:
#      trigger-keywords: keyword1, keyword2, keyword3
#
# Usage:
#   ./skill-index-builder.sh
#
# Exit codes:
#   0 - Index built successfully
#   1 - Error building index

set -euo pipefail

SKILLS_DIR="${HOME}/.claude/skills"
OUTPUT_DIR="${HOME}/.claude/hooks/loom"
OUTPUT_FILE="${OUTPUT_DIR}/skill-keywords.json"

# Ensure output directory exists
mkdir -p "$OUTPUT_DIR"

# Check if skills directory exists
if [[ ! -d "$SKILLS_DIR" ]]; then
    echo "Skills directory not found: $SKILLS_DIR" >&2
    exit 1
fi

# Temporary file for building the index
temp_index=$(mktemp)
trap 'rm -f "$temp_index"' EXIT

# Initialize empty object
echo "{}" > "$temp_index"

# Process each SKILL.md file
while IFS= read -r -d '' skill_file; do
    # Extract skill name from directory
    skill_dir=$(dirname "$skill_file")
    skill_name=$(basename "$skill_dir")

    # Extract YAML frontmatter (between --- markers)
    frontmatter=""
    in_frontmatter=false
    frontmatter_end=false

    while IFS= read -r line; do
        if [[ "$line" == "---" ]]; then
            if [[ "$in_frontmatter" == "true" ]]; then
                frontmatter_end=true
                break
            else
                in_frontmatter=true
                continue
            fi
        fi

        if [[ "$in_frontmatter" == "true" ]]; then
            frontmatter+="$line"$'\n'
        fi
    done < "$skill_file"

    if [[ "$frontmatter_end" != "true" ]]; then
        # No valid frontmatter found, skip
        continue
    fi

    # Extract triggers from frontmatter
    triggers=()

    # Method 1: YAML list format (triggers:)
    in_triggers=false
    while IFS= read -r line; do
        # Check if we're entering triggers section
        if [[ "$line" =~ ^triggers: ]]; then
            in_triggers=true
            continue
        fi

        # If we're in triggers section
        if [[ "$in_triggers" == "true" ]]; then
            # Check if line is a list item (starts with - after whitespace)
            if [[ "$line" =~ ^[[:space:]]*-[[:space:]]*(.*) ]]; then
                trigger="${BASH_REMATCH[1]}"
                # Trim whitespace
                trigger=$(echo "$trigger" | sed 's/^[[:space:]]*//;s/[[:space:]]*$//')
                if [[ -n "$trigger" ]]; then
                    triggers+=("$trigger")
                fi
            elif [[ "$line" =~ ^[[:space:]]*$ ]]; then
                # Empty line, continue
                continue
            elif [[ ! "$line" =~ ^[[:space:]] ]]; then
                # Non-indented line, end of triggers section
                in_triggers=false
            fi
        fi
    done <<< "$frontmatter"

    # Method 2: Comma-separated format (trigger-keywords:)
    if trigger_line=$(echo "$frontmatter" | grep -E '^trigger-keywords:' 2>/dev/null); then
        # Extract the value after the colon
        keywords="${trigger_line#*:}"
        # Split by comma and add each keyword
        IFS=',' read -ra keyword_array <<< "$keywords"
        for keyword in "${keyword_array[@]}"; do
            # Trim whitespace
            keyword=$(echo "$keyword" | sed 's/^[[:space:]]*//;s/[[:space:]]*$//')
            if [[ -n "$keyword" ]]; then
                triggers+=("$keyword")
            fi
        done
    fi

    # Add triggers to index
    for trigger in "${triggers[@]}"; do
        # Normalize: lowercase
        normalized=$(echo "$trigger" | tr '[:upper:]' '[:lower:]')

        # Skip empty triggers
        if [[ -z "$normalized" ]]; then
            continue
        fi

        # Escape for JSON (handle quotes and backslashes)
        normalized="${normalized//\\/\\\\}"
        normalized="${normalized//\"/\\\"}"

        # Read current index
        current_index=$(cat "$temp_index")

        # Check if keyword exists and add skill to array
        if echo "$current_index" | grep -q "\"$normalized\""; then
            # Keyword exists, add skill to array if not already present
            if ! echo "$current_index" | grep -q "\"$normalized\".*\"$skill_name\""; then
                # Add skill to existing array using jq-like logic with sed
                # This is a simplified approach - append skill before closing bracket
                current_index=$(echo "$current_index" | sed "s/\"$normalized\": \[\([^]]*\)\]/\"$normalized\": [\1, \"$skill_name\"]/")
                echo "$current_index" > "$temp_index"
            fi
        else
            # New keyword, create entry
            if [[ "$current_index" == "{}" ]]; then
                echo "{\"$normalized\": [\"$skill_name\"]}" > "$temp_index"
            else
                # Insert new entry before final closing brace
                current_index="${current_index%\}}"
                current_index="${current_index}, \"$normalized\": [\"$skill_name\"]}"
                echo "$current_index" > "$temp_index"
            fi
        fi
    done

done < <(find "$SKILLS_DIR" -name "SKILL.md" -print0 2>/dev/null)

# Write final output
cp "$temp_index" "$OUTPUT_FILE"

# Count skills and keywords
skill_count=$(find "$SKILLS_DIR" -name "SKILL.md" 2>/dev/null | wc -l | tr -d ' ')
keyword_count=$(grep -o '"[^"]*":' "$OUTPUT_FILE" 2>/dev/null | wc -l | tr -d ' ')

echo "Built skill keyword index: $keyword_count keywords from $skill_count skills"
echo "Output: $OUTPUT_FILE"
