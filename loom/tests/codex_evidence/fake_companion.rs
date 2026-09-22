pub const SCRIPT: &str = r#"import { createHash } from "node:crypto";
import fs from "node:fs";
import path from "node:path";

const argv = process.argv.slice(2);
const command = argv[0];
const workspaceRoot = fs.realpathSync.native(process.cwd());
const jobId = process.env.FAKE_CODEX_JOB_ID ?? "job-fixture";
const callsPath = process.env.FAKE_CODEX_CALLS;

function option(name, fallback = "") {
  const index = argv.indexOf(name);
  return index >= 0 ? argv[index + 1] : fallback;
}

function stateDir() {
  const slugSource = path.basename(workspaceRoot) || "workspace";
  const slug = slugSource.replace(/[^a-zA-Z0-9._-]+/g, "-")
    .replace(/^-+|-+$/g, "") || "workspace";
  const hash = createHash("sha256").update(workspaceRoot).digest("hex").slice(0, 16);
  return path.join(process.env.CLAUDE_PLUGIN_DATA, "state", `${slug}-${hash}`);
}

function jobPath(id) {
  return path.join(stateDir(), "jobs", `${id}.json`);
}

function recordCall() {
  if (!callsPath) return;
  const row = {
    command,
    args: argv.slice(1),
    sessionId: process.env.CODEX_COMPANION_SESSION_ID ?? null,
    pluginData: process.env.CLAUDE_PLUGIN_DATA ?? null
  };
  fs.appendFileSync(callsPath, `${JSON.stringify(row)}\n`, "utf8");
}

function phase(status) {
  return {
    queued: "queued",
    running: "starting",
    completed: "done",
    failed: "failed",
    cancelled: "cancelled"
  }[status] ?? "unknown";
}

function terminal(status) {
  return ["completed", "failed", "cancelled"].includes(status);
}

function buildJob() {
  const status = process.env.FAKE_CODEX_STATUS ?? "running";
  const model = option("--model", "gpt-6-sol");
  const effort = option("--effort", "xhigh");
  const prompt = argv[1] ?? "fixture prompt";
  const isTerminal = terminal(status);
  return {
    id: jobId,
    kind: "task",
    kindLabel: "Task",
    title: "Codex Task",
    workspaceRoot,
    jobClass: "task",
    summary: "Codex Task",
    write: argv.includes("--write"),
    createdAt: "2026-09-14T10:00:00.000Z",
    updatedAt: "2026-09-14T10:01:00.000Z",
    sessionId: process.env.CODEX_COMPANION_SESSION_ID,
    status,
    phase: phase(status),
    threadId: isTerminal ? (process.env.FAKE_CODEX_THREAD_ID ?? "thread-fixture") : null,
    turnId: isTerminal ? (process.env.FAKE_CODEX_TURN_ID ?? "turn-fixture") : null,
    completedAt: isTerminal ? "2026-09-14T10:01:00.000Z" : null,
    errorMessage: status === "failed" ? "companion failed" :
      status === "cancelled" ? "companion cancelled" : null,
    result: isTerminal ? { text: "terminal fixture" } : null,
    rendered: isTerminal ? "terminal fixture" : null,
    request: {
      cwd: workspaceRoot,
      model,
      effort,
      prompt,
      write: argv.includes("--write"),
      resumeLast: false,
      jobId
    }
  };
}

function writeJob(job) {
  fs.mkdirSync(path.dirname(jobPath(job.id)), { recursive: true });
  fs.writeFileSync(jobPath(job.id), `${JSON.stringify(job, null, 2)}\n`, "utf8");
}

function readJob(id) {
  return JSON.parse(fs.readFileSync(jobPath(id), "utf8"));
}

recordCall();
if (command === "task") {
  const job = buildJob();
  writeJob(job);
  console.log(JSON.stringify({ jobId, status: "queued", title: job.title, summary: job.summary }));
} else if (command === "status") {
  const id = argv[1];
  const job = readJob(id);
  const waitTimedOut = job.status === "queued" || job.status === "running";
  console.log(JSON.stringify({ workspaceRoot, job, waitTimedOut, timeoutMs: 540000 }));
} else if (command === "result") {
  const id = argv[1];
  const job = readJob(id);
  console.log(JSON.stringify({ job, storedJob: job }));
} else if (command === "cancel") {
  const id = argv[1];
  const job = readJob(id);
  if (!terminal(job.status)) {
    job.status = "cancelled";
    job.phase = "cancelled";
    job.pid = null;
    job.threadId = process.env.FAKE_CODEX_THREAD_ID ?? "thread-fixture";
    job.turnId = process.env.FAKE_CODEX_TURN_ID ?? "turn-fixture";
    job.completedAt = "2026-09-14T10:01:00.000Z";
    job.errorMessage = "Cancelled by user.";
    writeJob(job);
  }
  console.log(JSON.stringify({ job: { id: job.id, status: job.status, phase: job.phase } }));
} else {
  process.exitCode = 94;
}
"#;
