import os
import pathlib
import socket
import subprocess
import tempfile
import threading
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[2]


def block(path, start, end):
    text = (ROOT / path).read_text()
    return text.split(start, 1)[1].split(end, 1)[0]


class TerminalReaderTest(unittest.TestCase):
    def test_truncated_and_complete_responses(self):
        reader = block("scripts/smoke-web-terminal.sh", "python3 <(cat <<'PY'\n", "\nPY\n)")
        handshake = b"HTTP/1.1 101 Switching Protocols\r\n\r\n"
        for response, succeeds in [(b"", False), (b"HTTP/1.1 101", False),
                                   (handshake + b"\x88", False),
                                   (handshake + b"\x88\x02\x0f", False),
                                   (handshake + b"\x88\x02\x0f\xa4", True)]:
            with self.subTest(response=response), socket.socket() as server:
                server.bind(("127.0.0.1", 0))
                server.listen()
                port = server.getsockname()[1]

                def serve():
                    conn, _ = server.accept()
                    with conn:
                        conn.recv(4096)
                        conn.sendall(response)

                worker = threading.Thread(target=serve)
                worker.start()
                result = subprocess.run(["python3", "-c", reader, str(port), "origin"],
                                        input="cookie\n", text=True, capture_output=True, timeout=4)
                worker.join(timeout=4)
                self.assertEqual(result.returncode == 0, succeeds, result.stderr)


class SkillExampleTest(unittest.TestCase):
    def test_joinset_propagates_child_error(self):
        example = block("skills/loom-concurrency/SKILL.md", "```rust\n// Rust tokio: JoinSet", "\n```")
        source = """
use std::{future::Future, pin::pin, sync::Arc, task::{Context, Poll, Wake, Waker}};
mod tokio { pub mod task {
    pub struct JoinSet(Vec<Result<Result<(), &'static str>, &'static str>>);
    impl JoinSet {
        pub fn new() -> Self { Self(Vec::new()) }
        pub fn spawn(&mut self, result: Result<(), &'static str>) { self.0.push(Ok(result)); }
        pub async fn join_next(&mut self) -> Option<Result<Result<(), &'static str>, &'static str>> { self.0.pop() }
    }
} }
fn fetch(_: ()) -> Result<(), &'static str> { Err("child failed") }
async fn run() -> Result<(), &'static str> {
    let urls = [()];
""" + "\n// Rust tokio: JoinSet" + example + """
    Ok(())
}
struct Noop;
impl Wake for Noop { fn wake(self: Arc<Self>) {} }
fn main() {
    let waker = Waker::from(Arc::new(Noop));
    let mut future = pin!(run());
    assert_eq!(future.as_mut().poll(&mut Context::from_waker(&waker)), Poll::Ready(Err("child failed")));
}
"""
        with tempfile.TemporaryDirectory() as tmp:
            src, exe = pathlib.Path(tmp) / "test.rs", pathlib.Path(tmp) / "test"
            src.write_text(source)
            subprocess.run(["rustc", "--edition=2021", str(src), "-o", str(exe)], check=True)
            subprocess.run([str(exe)], check=True)

    def test_joinset_with_real_tokio_propagates_child_error(self):
        example = block("skills/loom-concurrency/SKILL.md", "```rust\n// Rust tokio: JoinSet", "\n```")
        source = """
use std::error::Error;
use std::io;

async fn fetch(_: ()) -> io::Result<()> {
    Err(io::Error::other("child failed"))
}

async fn run() -> Result<(), Box<dyn Error + Send + Sync>> {
    let urls = [()];
""" + "\n// Rust tokio: JoinSet" + example + """
    Ok(())
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    assert_eq!(run().await.unwrap_err().to_string(), "child failed");
}
"""
        with tempfile.TemporaryDirectory() as tmp:
            project = pathlib.Path(tmp) / "joinset_example"
            subprocess.run(["cargo", "new", "--bin", "--quiet", str(project)], check=True)
            subprocess.run(["cargo", "add", "tokio@1", "--features", "rt,macros", "--offline"],
                           cwd=project, check=True)
            (project / "src/main.rs").write_text(source)
            env = {**os.environ, "TMPDIR": str(project)}
            subprocess.run(["cargo", "run", "--offline", "--quiet"],
                           cwd=project, env=env, check=True)

    def test_c_growth_rejects_overflow_before_realloc(self):
        example = block("skills/loom-c/SKILL.md", "```c\n#include <stdint.h>\n#include <stdlib.h>\n\nint *grow", "\n```")
        source = "#include <stdint.h>\n#include <stdlib.h>\nint *grow" + example
        source += """
int main(void) {
    size_t cap = SIZE_MAX / 2 + 1;
    int value = 7;
    if (grow(&value, &cap) != NULL || value != 7 || cap != SIZE_MAX / 2 + 1) return 1;
    cap = 0;
    int *p = grow(NULL, &cap);
    if (!p || cap != 16) return 2;
    free(p);
    return 0;
}
"""
        with tempfile.TemporaryDirectory() as tmp:
            src, exe = pathlib.Path(tmp) / "test.c", pathlib.Path(tmp) / "test"
            src.write_text(source)
            subprocess.run(["cc", "-Wall", "-Werror", str(src), "-o", str(exe)], check=True)
            subprocess.run([str(exe)], check=True)

    def test_refresh_replay_revokes_replacement(self):
        example = block("skills/loom-auth/SKILL.md", "```typescript\n// Illustrative transaction API", "\n```")
        setup = """
class UnauthorizedError extends Error {}
const hashToken = x => x;
let serial = 0;
const generateSecureToken = () => `next${++serial}`;
const signToken = x => x;
const family = {userId: 'u', revoked: false};
const tokens = new Map([['original', {id: 'original', expiresAt: new Date(Date.now()+60000), consumed: false}]]);
const tx = {
  findFamilyIdByTokenHash: async h => tokens.has(h) ? 'family' : null,
  lockFamily: async () => family,
  findTokenByHash: async h => tokens.get(h),
  revokeFamily: async () => { family.revoked = true; },
  markConsumed: async id => { tokens.get(id).consumed = true; },
  insertToken: async ({hash, expiresAt}) => tokens.set(hash, {id: hash, expiresAt, consumed: false}),
};
const db = { transaction: async f => f(tx) };
"""
        check = """
const replacement = (await refreshTokens('original')).refreshToken;
for (const token of ['original', replacement]) {
  let rejected = false;
  try { await refreshTokens(token); } catch (e) { if (!(e instanceof UnauthorizedError)) throw e; rejected = true; }
  if (!rejected) throw new Error('replay or replacement accepted');
}
if (!family.revoked) throw new Error('family not revoked');
"""
        with tempfile.TemporaryDirectory() as tmp:
            script = pathlib.Path(tmp) / "example.ts"
            script.write_text(setup + "\n// Illustrative transaction API" + example + check)
            subprocess.run(["bun", str(script)], check=True)


if __name__ == "__main__":
    unittest.main()
