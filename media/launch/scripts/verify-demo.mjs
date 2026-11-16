import assert from "node:assert/strict";
import { execFileSync, spawn } from "node:child_process";
import { mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { createInterface } from "node:readline";

const binary = resolve(process.env.PGS_MCP_BINARY ?? "../../target/debug/pgs-mcp");
const repo = mkdtempSync(join(tmpdir(), "pgs-film-"));
const evidence = resolve("out/evidence/demo");
mkdirSync(evidence, { recursive: true });
const git = (...args) => execFileSync("git", ["-C", repo, ...args], { encoding: "utf8" });
const original = 'fn is_valid(token: &str) -> bool {\n    token.len() > 0\n}\n\nfn main() {\n    run();\n}\n';
const committed = original.replace("token.len() > 0", "!token.trim().is_empty()");
const working = committed.replace("    run();", '    eprintln!("debug: auth check");\n    run();');
const server = spawn(binary, [], { stdio: ["pipe", "pipe", "inherit"] });
const lines = createInterface({ input: server.stdout })[Symbol.asyncIterator]();
let id = 0;
const timeout = setTimeout(() => server.kill(), 30000);

async function request(method, params) {
  const requestId = ++id;
  server.stdin.write(`${JSON.stringify({ jsonrpc: "2.0", id: requestId, method, params })}\n`);
  for (;;) {
    const line = await lines.next();
    assert.equal(line.done, false, "MCP server closed before replying");
    const reply = JSON.parse(line.value);
    if (reply.id !== requestId) continue;
    assert.equal(reply.error, undefined, JSON.stringify(reply.error));
    return reply.result;
  }
}
async function call(name, args) {
  const result = await request("tools/call", { name, arguments: { repo_path: repo, ...args } });
  assert.equal(result.isError, false, JSON.stringify(result));
  writeFileSync(join(evidence, `${id}-${name}.json`), JSON.stringify(result, null, 2));
  return result.structuredContent.pgs;
}
try {
  git("init", "-q");
  git("config", "user.name", "pgs demo");
  git("config", "user.email", "demo@example.invalid");
  git("config", "core.autocrlf", "false");
  git("config", "core.hooksPath", join(repo, "empty-hooks"));
  git("config", "commit.gpgsign", "false");
  mkdirSync(join(repo, "src"));
  writeFileSync(join(repo, "src/auth.rs"), original);
  git("add", ".");
  git("commit", "-qm", "chore: initial example");
  writeFileSync(join(repo, "src/auth.rs"), working);
  await request("initialize", { protocolVersion: "2025-11-25", capabilities: {}, clientInfo: { name: "pgs-film-verification", version: "1.0.0" } });
  server.stdin.write(`${JSON.stringify({ jsonrpc: "2.0", method: "notifications/initialized" })}\n`);
  const scan = await call("pgs_scan", { files: ["src/auth.rs"], full: true });
  assert.equal(scan.files[0].hunks[0].id, "d46b3c1640d6");
  const preview = await call("pgs_stage", { selections: ["src/auth.rs:2-2"], dry_run: true, explain: true });
  assert.deepEqual(preview.previews[0].preview_lines.map((line) => line.content), ["    !token.trim().is_empty()"]);
  assert.equal(git("show", ":src/auth.rs"), original);
  await call("pgs_stage", { selections: ["src/auth.rs:2-2"] });
  const staged = git("diff", "--cached");
  assert.match(staged, /\+    !token.trim\(\).is_empty\(\)/);
  assert.match(staged, /-    token.len\(\) > 0/);
  assert.doesNotMatch(staged, /eprintln/);
  writeFileSync(join(evidence, "staged.diff"), staged);
  await call("pgs_commit", { message: "fix: reject blank tokens" });
  assert.equal(git("show", "HEAD:src/auth.rs"), committed);
  assert.equal(readFileSync(join(repo, "src/auth.rs"), "utf8"), working);
  const remaining = await call("pgs_scan", { files: ["src/auth.rs"], full: true });
  assert.deepEqual(remaining.files[0].hunks.flatMap((hunk) => hunk.lines).filter((line) => line.origin === "Addition").map((line) => line.content), ['    eprintln!("debug: auth check");']);
  writeFileSync(join(evidence, "committed.rs"), committed);
  writeFileSync(join(evidence, "unstaged.diff"), git("diff"));
  console.log("PASS: actual MCP scan → preview → stage → commit → rescan; only fix committed, debug remains local");
} finally {
  clearTimeout(timeout);
  server.stdin.end();
  server.kill();
  await lines.return();
  rmSync(repo, { recursive: true, force: true });
}
