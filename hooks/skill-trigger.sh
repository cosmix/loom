#!/usr/bin/env bash
# skill-trigger.sh - UserPromptSubmit hook for keyword-based skill suggestions
#
# This hook reads user prompts and suggests relevant skills based on keyword matching.
#
# Input: JSON from stdin with user prompt
#   {"session_id": "...", "message": "Help me implement JWT authentication"}
#
# Output: Skill suggestions to stdout (injected into context)
#   SKILL SUGGESTIONS: Based on your prompt, consider:
#   - /auth - Authentication patterns (matched: "JWT", "authentication")
#   - /testing - Test implementation (matched: "implement")
#
# Environment variables:
#   None required
#
# Exit codes:
#   0 - Continue with suggestions in stdout (context injection)
#   0 - Continue without suggestions (no matches)
#
# Dependencies:
#   ~/.claude/hooks/loom/skill-keywords.json (built by skill-index-builder.sh)

set -euo pipefail

INDEX_FILE="${HOME}/.claude/hooks/loom/skill-keywords.json"
MATCH_THRESHOLD=2  # Minimum score to suggest a skill
MAX_SUGGESTIONS=3  # Maximum number of skills to suggest

# Read JSON input from stdin
input=""
if [[ ! -t 0 ]]; then
    input=$(cat)
fi

# Exit silently if no input
if [[ -z "$input" ]]; then
    exit 0
fi

# Check if index file exists
if [[ ! -f "$INDEX_FILE" ]]; then
    exit 0
fi

# Extract message from JSON input
# Handle both "message" and "prompt" fields
message=""
if echo "$input" | grep -q '"message"'; then
    message=$(echo "$input" | sed -n 's/.*"message"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p')
fi
if [[ -z "$message" ]] && echo "$input" | grep -q '"prompt"'; then
    message=$(echo "$input" | sed -n 's/.*"prompt"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p')
fi

# Exit if no message found
if [[ -z "$message" ]]; then
    exit 0
fi

# Read the keyword index
index_content=$(cat "$INDEX_FILE")

# Tokenize and normalize the message (lowercase, split on whitespace/punctuation)
# Use tr to lowercase and split on common delimiters
tokens=$(echo "$message" | tr '[:upper:]' '[:lower:]' | tr -cs '[:alnum:]' '\n' | sort -u)

# Create associative array for skill scores and matched keywords
declare -A skill_scores
declare -A skill_matches

# Match tokens against keyword index
while IFS= read -r token; do
    # Skip empty tokens or very short ones
    if [[ -z "$token" ]] || [[ ${#token} -lt 2 ]]; then
        continue
    fi

    # Check if token exists in index (exact match)
    # Extract skills for this keyword using grep/sed
    if echo "$index_content" | grep -q "\"$token\"[[:space:]]*:"; then
        # Extract the skill array for this keyword
        skills_json=$(echo "$index_content" | grep -o "\"$token\"[[:space:]]*:[[:space:]]*\[[^]]*\]" | sed 's/.*\[\([^]]*\)\].*/\1/')

        # Parse skill names from the array
        while IFS= read -r skill; do
            # Clean up the skill name (remove quotes and whitespace)
            skill=$(echo "$skill" | sed 's/[",[:space:]]//g')
            if [[ -n "$skill" ]]; then
                # Increment score
                current_score="${skill_scores[$skill]:-0}"
                skill_scores[$skill]=$((current_score + 1))

                # Track matched keywords
                current_matches="${skill_matches[$skill]:-}"
                if [[ -z "$current_matches" ]]; then
                    skill_matches[$skill]="$token"
                else
                    skill_matches[$skill]="$current_matches, $token"
                fi
            fi
        done <<< "$(echo "$skills_json" | tr ',' '\n')"
    fi

    # Also check for partial matches (token is substring of keyword or vice versa)
    # This helps catch variations like "testing" matching "test"
    while IFS= read -r keyword_entry; do
        keyword=$(echo "$keyword_entry" | sed 's/"\([^"]*\)":.*/\1/')
        if [[ -z "$keyword" ]]; then
            continue
        fi

        # Check if token is a prefix of keyword (at least 3 chars) or keyword is prefix of token
        if [[ ${#token} -ge 3 ]]; then
            if [[ "$keyword" == "$token"* ]] || [[ "$token" == "$keyword"* ]]; then
                # Skip if we already matched this keyword exactly
                if [[ "$keyword" != "$token" ]]; then
                    # Extract skills for this keyword
                    skills_json=$(echo "$index_content" | grep -o "\"$keyword\"[[:space:]]*:[[:space:]]*\[[^]]*\]" | sed 's/.*\[\([^]]*\)\].*/\1/')

                    while IFS= read -r skill; do
                        skill=$(echo "$skill" | sed 's/[",[:space:]]//g')
                        if [[ -n "$skill" ]]; then
                            # Add partial match with lower weight (0.5)
                            current_score="${skill_scores[$skill]:-0}"
                            # For bash integer math, add 1 for every 2 partial matches
                            if (( (current_score % 2) == 0 )); then
                                skill_scores[$skill]=$((current_score + 1))
                            fi

                            # Track matched keywords (mark as partial)
                            current_matches="${skill_matches[$skill]:-}"
                            if [[ -z "$current_matches" ]]; then
                                skill_matches[$skill]="$token~$keyword"
                            elif [[ "$current_matches" != *"$keyword"* ]]; then
                                skill_matches[$skill]="$current_matches, $token~$keyword"
                            fi
                        fi
                    done <<< "$(echo "$skills_json" | tr ',' '\n')"
                fi
            fi
        fi
    done <<< "$(echo "$index_content" | grep -o '"[^"]*":' | sed 's/:$//')"

done <<< "$tokens"

# Sort skills by score and collect top suggestions
suggestions=()
suggestion_count=0

# Sort skills by score (descending)
for skill in "${!skill_scores[@]}"; do
    score="${skill_scores[$skill]}"
    if (( score >= MATCH_THRESHOLD )); then
        suggestions+=("$score:$skill")
    fi
done

# Sort by score (numeric, descending)
IFS=$'\n' sorted_suggestions=($(sort -t: -k1 -rn <<< "${suggestions[*]:-}"))
unset IFS

# Build output
output=""
for suggestion in "${sorted_suggestions[@]:-}"; do
    if (( suggestion_count >= MAX_SUGGESTIONS )); then
        break
    fi

    score="${suggestion%%:*}"
    skill="${suggestion#*:}"
    matches="${skill_matches[$skill]:-}"

    # Clean up matches (take first 3)
    match_list=$(echo "$matches" | tr ',' '\n' | head -3 | tr '\n' ',' | sed 's/,$//' | sed 's/,/, /g')

    # Get skill description from SKILL.md if available
    skill_file="${HOME}/.claude/skills/${skill}/SKILL.md"
    description=""
    if [[ -f "$skill_file" ]]; then
        # Extract description from frontmatter
        description=$(sed -n '/^---$/,/^---$/p' "$skill_file" | grep -E '^description:' | sed 's/^description:[[:space:]]*//' | head -1)
        # Truncate to first sentence or 80 chars
        description=$(echo "$description" | cut -c1-80 | sed 's/\. .*/\./')
    fi

    if [[ -z "$description" ]]; then
        description="(matched: $match_list)"
    else
        description="$description (matched: $match_list)"
    fi

    output+="- /${skill} - ${description}"$'\n'
    ((suggestion_count++))
done

# Output suggestions if any
if [[ -n "$output" ]] && (( suggestion_count > 0 )); then
    echo "SKILL SUGGESTIONS: Based on your prompt, consider using:"
    echo "$output"
    echo "Use Skill(skill_name) to invoke the most relevant skill."
fi

exit 0
