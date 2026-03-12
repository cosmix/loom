#!/usr/bin/env python3
"""skill-trigger - UserPromptSubmit hook for keyword-based skill suggestions.

Reads user prompts via stdin JSON and outputs skill suggestions to stdout.
Suggestions are injected into Claude's context to encourage skill usage.

Input: JSON from stdin with structure:
  {"session_id": "...", "prompt": "Help me implement JWT authentication"}

Output: Plain text skill suggestions to stdout (context injection).

Dependencies:
  ~/.claude/hooks/loom/skill-keywords.json (built by skill-index-builder.sh)
"""

import json
import os
import re
import sys

INDEX_FILE = os.path.expanduser("~/.claude/hooks/loom/skill-keywords.json")
SKILLS_DIR = os.path.expanduser("~/.claude/skills")
MAX_SUGGESTIONS = 3


def main():
    if sys.stdin.isatty():
        return

    try:
        data = json.load(sys.stdin)
    except (json.JSONDecodeError, ValueError):
        return

    prompt = data.get("prompt", "")
    if not prompt or not os.path.isfile(INDEX_FILE):
        return

    try:
        with open(INDEX_FILE) as f:
            index = json.load(f)
    except (json.JSONDecodeError, IOError):
        return

    # Tokenize: lowercase, keep special chars like / - . within words
    words = re.findall(r"[a-z0-9]+(?:[/._-][a-z0-9]+)*", prompt.lower())
    tokens = set(w for w in words if len(w) > 1)

    # Generate bigrams and trigrams for multi-word keyword matching
    # e.g. "event sourcing", "api key", "access control"
    for i in range(len(words) - 1):
        tokens.add(f"{words[i]} {words[i + 1]}")
    for i in range(len(words) - 2):
        tokens.add(f"{words[i]} {words[i + 1]} {words[i + 2]}")

    # Score skills by keyword matches
    scores = {}
    matched = {}
    for token in tokens:
        if token in index:
            for skill in index[token]:
                scores[skill] = scores.get(skill, 0) + 1
                matched.setdefault(skill, []).append(token)

    if not scores:
        return

    # Sort by score descending, take top N
    top = sorted(scores.items(), key=lambda x: -x[1])[:MAX_SUGGESTIONS]

    lines = []
    for skill, _score in top:
        kws = ", ".join(matched[skill][:4])
        desc = _get_description(skill)
        if desc:
            lines.append(f"  - /{skill} -- {desc} (matched: {kws})")
        else:
            lines.append(f"  - /{skill} (matched: {kws})")

    if lines:
        print(
            "SKILL MATCH: These skills are relevant to this task."
            ' Invoke the best match with Skill(skill="name") before implementing:'
        )
        for line in lines:
            print(line)


def _get_description(skill_name):
    """Extract short description from SKILL.md frontmatter."""
    path = os.path.join(SKILLS_DIR, skill_name, "SKILL.md")
    if not os.path.isfile(path):
        return ""
    try:
        with open(path) as f:
            text = f.read(2000)
        m = re.search(r"^---\s*\n(.*?)\n---", text, re.DOTALL)
        if not m:
            return ""
        fm = m.group(1)
        # Multiline description (|)
        m = re.search(r"^description:\s*\|\s*\n\s+(.+)", fm, re.MULTILINE)
        if m:
            d = m.group(1).strip()
        else:
            # Inline description
            m = re.search(r"^description:\s*(.+)", fm, re.MULTILINE)
            if not m:
                return ""
            d = m.group(1).strip()
        # Truncate at first natural break point
        for marker in [". Trigger", ". Use when", ". Covers", ". Keywords", ". Primary"]:
            idx = d.find(marker)
            if 0 < idx < 80:
                d = d[: idx + 1]
                break
        return d[:80]
    except IOError:
        return ""


if __name__ == "__main__":
    main()
