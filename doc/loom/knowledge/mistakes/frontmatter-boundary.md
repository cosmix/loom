# Frontmatter Boundary

> Keep YAML and body offsets distinct when sharing frontmatter boundaries

## Shadowed frontmatter body offset

**What happened:** While changing the frontmatter splitter to return both YAML and body, a newly named body offset shadowed the YAML start offset and briefly made the returned YAML slice empty.

**Why:** Two distinct offsets were represented by the same local name during the first edit.

**Prevention:** Keep the YAML start and post-delimiter body start as separate named offsets; test both returned slices with indented delimiters and CRLF.

**Fix:** Renamed the post-delimiter offset to `remainder_start`, retained the original `body_start` for YAML, and added wrapper boundary tests.
