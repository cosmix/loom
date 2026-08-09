pub fn sanitize(source: &str) -> Result<Vec<u8>, String> {
    let bytes = source.as_bytes();
    let mut sanitized = bytes.to_vec();
    let mut cursor = 0;
    while cursor < bytes.len() {
        if let Some(end) = masked_span(bytes, cursor)? {
            mask_non_newlines(&mut sanitized[cursor..end]);
            cursor = end;
        } else {
            cursor += 1;
        }
    }
    Ok(sanitized)
}

fn masked_span(source: &[u8], start: usize) -> Result<Option<usize>, String> {
    if source[start..].starts_with(b"//") {
        return Ok(Some(line_comment_end(source, start + 2)));
    }
    if source[start..].starts_with(b"/*") {
        return block_comment_end(source, start).map(Some);
    }
    if let Some(end) = raw_string_end(source, start)? {
        return Ok(Some(end));
    }
    if source[start] == b'"' {
        return quoted_end(source, start, b'"').map(Some);
    }
    if source[start] == b'\'' {
        return char_literal_end(source, start);
    }
    Ok(None)
}

fn line_comment_end(source: &[u8], start: usize) -> usize {
    source[start..]
        .iter()
        .position(|byte| *byte == b'\n')
        .map_or(source.len(), |offset| start + offset)
}

fn block_comment_end(source: &[u8], start: usize) -> Result<usize, String> {
    let mut cursor = start + 2;
    let mut depth = 1usize;
    while cursor + 1 < source.len() {
        match &source[cursor..cursor + 2] {
            b"/*" => {
                depth += 1;
                cursor += 2;
            }
            b"*/" => {
                depth -= 1;
                cursor += 2;
                if depth == 0 {
                    return Ok(cursor);
                }
            }
            _ => cursor += 1,
        }
    }
    Err("unclosed block comment while scanning Rust source".to_string())
}

fn quoted_end(source: &[u8], start: usize, delimiter: u8) -> Result<usize, String> {
    let mut cursor = start + 1;
    while cursor < source.len() {
        if source[cursor] == b'\\' {
            cursor = (cursor + 2).min(source.len());
        } else if source[cursor] == delimiter {
            return Ok(cursor + 1);
        } else {
            cursor += 1;
        }
    }
    Err(format!("unclosed quoted literal at byte {start}"))
}

fn char_literal_end(source: &[u8], start: usize) -> Result<Option<usize>, String> {
    let mut cursor = start + 1;
    if source.get(cursor) == Some(&b'\\') {
        cursor += 2;
        if source.get(cursor.wrapping_sub(1)) == Some(&b'u') && source.get(cursor) == Some(&b'{') {
            cursor += unicode_escape_end(source, cursor, start)?;
        }
    } else {
        let Some(character) = next_character(source, cursor) else {
            return Ok(None);
        };
        cursor += character.len_utf8();
    }
    Ok((source.get(cursor) == Some(&b'\'')).then_some(cursor + 1))
}

fn unicode_escape_end(source: &[u8], cursor: usize, start: usize) -> Result<usize, String> {
    source[cursor..]
        .iter()
        .position(|byte| *byte == b'}')
        .map(|offset| offset + 1)
        .ok_or_else(|| format!("unclosed Unicode character literal at byte {start}"))
}

fn next_character(source: &[u8], cursor: usize) -> Option<char> {
    std::str::from_utf8(&source[cursor..])
        .ok()
        .and_then(|rest| rest.chars().next())
}

fn raw_string_end(source: &[u8], start: usize) -> Result<Option<usize>, String> {
    if !is_token_boundary(source, start.checked_sub(1)) {
        return Ok(None);
    }
    let Some(mut cursor) = raw_prefix_end(source, start) else {
        return Ok(None);
    };
    let hash_start = cursor;
    while source.get(cursor) == Some(&b'#') {
        cursor += 1;
    }
    if source.get(cursor) != Some(&b'"') {
        return Ok(None);
    }
    let delimiter = raw_delimiter(cursor - hash_start);
    let content_start = cursor + 1;
    find_subslice(&source[content_start..], &delimiter)
        .map(|offset| Some(content_start + offset + delimiter.len()))
        .ok_or_else(|| format!("unclosed raw string at byte {start}"))
}

fn raw_prefix_end(source: &[u8], start: usize) -> Option<usize> {
    if source[start..].starts_with(b"br") || source[start..].starts_with(b"rb") {
        Some(start + 2)
    } else if source[start] == b'r' {
        Some(start + 1)
    } else {
        None
    }
}

fn is_token_boundary(source: &[u8], index: Option<usize>) -> bool {
    index
        .and_then(|value| source.get(value))
        .is_none_or(|byte| !byte.is_ascii_alphanumeric() && *byte != b'_')
}

fn raw_delimiter(hashes: usize) -> Vec<u8> {
    std::iter::once(b'"')
        .chain(std::iter::repeat_n(b'#', hashes))
        .collect()
}

fn find_subslice(source: &[u8], needle: &[u8]) -> Option<usize> {
    source
        .windows(needle.len())
        .position(|window| window == needle)
}

fn mask_non_newlines(span: &mut [u8]) {
    for byte in span {
        if *byte != b'\n' {
            *byte = b' ';
        }
    }
}
