//! Streaming ANSI escape-sequence filter (MN6).
//!
//! `loom container logs` pipes raw container output back to the user's
//! terminal. Container output is untrusted: an agent — or a buggy
//! application inside the container — can emit OSC 52 (clipboard write),
//! OSC 8 (hyperlink), DCS / APC / PM sequences, or bare BELs. These are
//! all terminal-injection vectors that a hostile container could use to
//! affect the host shell.
//!
//! This filter operates on raw bytes and:
//!
//!   * Preserves CSI SGR sequences only: `ESC [ <params; ...> m`
//!     (param bytes `0x30-0x3F` / `;`) — colours and text attributes.
//!   * Drops all other CSI sequences (cursor moves, scroll regions, etc.).
//!   * Drops OSC (`ESC ]`), DCS (`ESC P`), APC (`ESC _`), PM (`ESC ^`)
//!     entirely, including their string-terminator (ST or BEL).
//!   * Drops BEL (`0x07`), DEL (`0x7F`), and stray ESC.
//!   * Passes through every other byte unchanged, including control codes
//!     that are legitimately rendered (newline `0x0A`, carriage return
//!     `0x0D`, tab `0x09`).
//!
//! Sequences may be split across input chunks — the filter is a byte
//! state machine that buffers in-progress sequences between calls.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Normal,
    AfterEsc,
    InCsi,
    InString, // OSC / DCS / APC / PM — drop until ST or BEL
    AfterEscInString,
}

/// Streaming ANSI sanitizer.
pub struct AnsiFilter {
    state: State,
    /// Buffer of CSI parameter bytes seen since the last `ESC [`. Flushed
    /// to output only if the CSI is an SGR (terminator 'm'); discarded
    /// for every other terminator.
    csi_buf: Vec<u8>,
}

impl Default for AnsiFilter {
    fn default() -> Self {
        Self::new()
    }
}

impl AnsiFilter {
    pub fn new() -> Self {
        Self {
            state: State::Normal,
            csi_buf: Vec::new(),
        }
    }

    /// Feed a byte chunk through the filter. Returns the sanitized bytes.
    ///
    /// Trailing in-progress sequences are NOT flushed; they remain
    /// buffered for the next call. Use [`Self::finish`] to flush any
    /// truncated tail when input ends.
    pub fn feed(&mut self, input: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(input.len());
        for &b in input {
            self.step(b, &mut out);
        }
        out
    }

    /// Flush any in-progress sequence buffer that has no terminator. The
    /// dangling escape sequence is discarded; nothing is appended to
    /// output. Returns an empty Vec.
    pub fn finish(&mut self) -> Vec<u8> {
        // Truncated CSI / OSC / DCS sequences are dropped (better to lose
        // a few attribute bytes than to forward an unterminated escape
        // that the terminal will keep eating).
        self.state = State::Normal;
        self.csi_buf.clear();
        Vec::new()
    }

    fn step(&mut self, b: u8, out: &mut Vec<u8>) {
        match self.state {
            State::Normal => {
                if b == 0x1B {
                    self.state = State::AfterEsc;
                } else if b == 0x07 || b == 0x7F {
                    // BEL and DEL — drop.
                } else {
                    out.push(b);
                }
            }
            State::AfterEsc => {
                match b {
                    b'[' => {
                        self.state = State::InCsi;
                        self.csi_buf.clear();
                    }
                    b']' | b'P' | b'_' | b'^' => {
                        // OSC / DCS / APC / PM — enter string state, drop everything until ST or BEL.
                        self.state = State::InString;
                    }
                    _ => {
                        // Two-byte sequences (ESC c reset, ESC = appkeypad, etc.)
                        // and stray ESC — drop the lone ESC entirely.
                        self.state = State::Normal;
                    }
                }
            }
            State::InCsi => {
                if (0x30..=0x3F).contains(&b) {
                    // Parameter bytes (digits, ';', '?', etc.) — buffer.
                    self.csi_buf.push(b);
                } else if (0x20..=0x2F).contains(&b) {
                    // Intermediate bytes — buffer; SGR doesn't use these,
                    // so seeing one means we'll drop the whole sequence.
                    self.csi_buf.push(b);
                } else if (0x40..=0x7E).contains(&b) {
                    // Final byte. Allow ONLY 'm' (SGR). Everything else
                    // (cursor move, scroll, clear, …) is dropped silently.
                    if b == b'm' && !self.csi_buf.iter().any(|&p| !is_sgr_param(p)) {
                        // Reconstruct ESC [ <params> m.
                        out.push(0x1B);
                        out.push(b'[');
                        out.extend_from_slice(&self.csi_buf);
                        out.push(b'm');
                    }
                    self.csi_buf.clear();
                    self.state = State::Normal;
                } else {
                    // Out-of-spec byte inside CSI — abort sequence.
                    self.csi_buf.clear();
                    self.state = State::Normal;
                }
            }
            State::InString => {
                if b == 0x07 {
                    // BEL terminator — end string state.
                    self.state = State::Normal;
                } else if b == 0x1B {
                    self.state = State::AfterEscInString;
                }
                // All other bytes inside OSC/DCS/APC/PM are dropped.
            }
            State::AfterEscInString => {
                // Inside a string state, `ESC \` is the String Terminator (ST).
                // Anything else: stay in string state.
                if b == b'\\' {
                    self.state = State::Normal;
                } else {
                    self.state = State::InString;
                }
            }
        }
    }
}

/// SGR parameter bytes are restricted to digits `0-9` and `;`. We use the
/// stricter check (rather than the full 0x30–0x3F range) because real-world
/// SGR sequences never use `?`/`<`/`>`/`=` parameter bytes, and those
/// markers are reserved for private-use CSI variants the spec forbids us
/// from rewriting.
fn is_sgr_param(b: u8) -> bool {
    b.is_ascii_digit() || b == b';'
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(chunks: &[&[u8]]) -> Vec<u8> {
        let mut f = AnsiFilter::new();
        let mut out = Vec::new();
        for c in chunks {
            out.extend_from_slice(&f.feed(c));
        }
        out.extend_from_slice(&f.finish());
        out
    }

    #[test]
    fn passes_plain_text() {
        let out = run(&[b"hello world\n"]);
        assert_eq!(out, b"hello world\n");
    }

    #[test]
    fn preserves_sgr() {
        let out = run(&[b"\x1b[31mred\x1b[0m"]);
        assert_eq!(out, b"\x1b[31mred\x1b[0m");
    }

    #[test]
    fn preserves_sgr_multi_param() {
        let out = run(&[b"\x1b[1;33;44mok\x1b[0m"]);
        assert_eq!(out, b"\x1b[1;33;44mok\x1b[0m");
    }

    #[test]
    fn drops_non_sgr_csi() {
        // CSI cursor up — drop.
        let out = run(&[b"\x1b[2Aabc"]);
        assert_eq!(out, b"abc");
    }

    #[test]
    fn drops_osc_with_st() {
        // OSC 0 ; title BEL — drop entirely.
        let out = run(&[b"prefix\x1b]0;evil\x07suffix"]);
        assert_eq!(out, b"prefixsuffix");
    }

    #[test]
    fn drops_osc_with_st_escbackslash() {
        // OSC … ESC \ — drop entirely.
        let out = run(&[b"prefix\x1b]52;c;BASE64DATA\x1b\\suffix"]);
        assert_eq!(out, b"prefixsuffix");
    }

    #[test]
    fn drops_bel() {
        let out = run(&[b"a\x07b\x07c"]);
        assert_eq!(out, b"abc");
    }

    #[test]
    fn drops_lone_esc() {
        let out = run(&[b"\x1bx"]);
        // Lone ESC plus non-sequence char → drop both.
        assert_eq!(out, b"");
    }

    #[test]
    fn handles_split_csi_across_chunks() {
        // ESC [ 3 1 m on chunk boundary.
        let out = run(&[b"a\x1b[", b"31m", b"b"]);
        assert_eq!(out, b"a\x1b[31mb");
    }

    #[test]
    fn handles_split_osc_across_chunks() {
        let out = run(&[b"a\x1b]", b"0;t", b"itle\x07b"]);
        assert_eq!(out, b"ab");
    }

    #[test]
    fn drops_dcs() {
        // ESC P … ESC \\
        let out = run(&[b"a\x1bP1$rDCS\x1b\\b"]);
        assert_eq!(out, b"ab");
    }

    #[test]
    fn drops_apc() {
        let out = run(&[b"a\x1b_APC\x1b\\b"]);
        assert_eq!(out, b"ab");
    }

    #[test]
    fn drops_pm() {
        let out = run(&[b"a\x1b^PM\x1b\\b"]);
        assert_eq!(out, b"ab");
    }

    #[test]
    fn truncated_csi_is_dropped_on_finish() {
        // ESC [ 3 (no terminator) at end of stream → drop.
        let mut f = AnsiFilter::new();
        let part = f.feed(b"a\x1b[3");
        let tail = f.finish();
        let mut combined = part;
        combined.extend_from_slice(&tail);
        assert_eq!(combined, b"a");
    }

    #[test]
    fn passes_newline_tab_cr() {
        let out = run(&[b"a\nb\tc\rd"]);
        assert_eq!(out, b"a\nb\tc\rd");
    }

    #[test]
    fn rejects_csi_with_private_marker() {
        // CSI ? 1049 h — alternate screen buffer toggle; not SGR.
        let out = run(&[b"\x1b[?1049h"]);
        assert_eq!(out, b"");
    }
}
