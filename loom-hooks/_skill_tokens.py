"""Prompt tokenizer for skill-trigger.sh.

Turns a prompt into the set of strings that are looked up in the keyword
index: single words, phrases of up to MAX_PHRASE_WORDS words, and plural stems.
Every key the indexer can write must be reachable by feeding it back in as a
prompt; loom/tests/integration/hooks_skill_trigger_reach.rs enforces that.
"""

import re

# A phrase longer than this is never built, so a pathological index key cannot
# make the n-gram pass quadratic on a long prompt. A key with more words than
# this cannot match and should be rewritten shorter.
MAX_PHRASE_WORDS = 6

# A word is letters and digits, joined by one of / . _ - + : (only when
# another letter or digit follows). It may also start with a dot when the dot
# opens a token (".net", not "sentence.") and end with "++" or "#" ("c++",
# "c#"). Everything else, including a trailing "." or ":", separates words.
_WORD_RE = re.compile(
    r"""(?:(?<![^\s(\[{"'`,;])\.(?=[a-z]))?"""
    r"[a-z0-9]+(?:(?:[/._+-]|::?)[a-z0-9]+)*(?:\+\+|#)?"
)
_PART_SPLIT_RE = re.compile(r"[+:]+")


def _word_variants(word):
    """The word itself plus the forms a keyword could have been written in:
    without the leading dot or trailing marker (".env" -> "env", "c++" -> "c"),
    and the pieces of a "+" or ":" compound, cut at "/" first so that
    "application/problem+json" also yields "problem+json"."""
    yield word
    base = word.lstrip(".").rstrip("+#") if word[0] == "." or word[-1] in "+#" else word
    if base != word:
        yield base
    if _PART_SPLIT_RE.search(base):
        for piece in base.split("/"):
            yield piece
            if _PART_SPLIT_RE.search(piece):
                yield from _PART_SPLIT_RE.split(piece)


def _max_phrase_words(index):
    longest = max((key.count(" ") + 1 for key in index), default=1)
    return min(longest, MAX_PHRASE_WORDS)


def _stems(token, index):
    parts = token.split(" ")
    last = parts[-1]
    stems = []
    if last.endswith("s") and len(last) > 3:
        stems.append(last[:-1])
    if last.endswith("es") and len(last) > 4:
        stems.append(last[:-2])
    for stem in stems:
        candidate = " ".join(parts[:-1] + [stem])
        if candidate in index:
            return candidate
    return None


def tokenize(prompt, index, stopwords):
    words = _WORD_RE.findall(prompt.lower())
    tokens = {
        variant
        for word in words
        for variant in _word_variants(word)
        if len(variant) > 1 and (variant not in stopwords or variant in index)
    }
    for size in range(2, _max_phrase_words(index) + 1):
        tokens.update(" ".join(words[i:i + size]) for i in range(len(words) - size + 1))
    stemmed = {
        candidate for token in tokens - index.keys()
        if (candidate := _stems(token, index))
    }
    return tokens | stemmed
