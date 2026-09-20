// Verifies the TUI's JIRA browser handoff against a fake opener — no
// credentials, no network, and no real browser.
//
//   node scripts/verify-jira-open.mjs             # G1: `o` on a keyed task hands
//                                                 #     its issue URL to the opener,
//                                                 #     and an unkeyed task does not
//   node scripts/verify-jira-open.mjs --self-test # G2: the same oracle rejects a
//                                                 #     handoff that never happened
//
// The TUI draws to a terminal, so the session runs under a pty (`script`), the
// same way verify-status-picker.mjs drives it. `xdg-open` is shadowed by a stub
// early on PATH that appends its argv to a capture file, so the assertion reads
// the URL the app actually handed the opener rather than the source that builds
// it — and a task with no key must leave that file untouched.
//
// The sprint/fetch ingest half is not driven here: reaching it needs Ctrl+J,
// which a pty cannot distinguish from Enter. Its persistence is asserted
// directly by `ingesting_a_jira_issue_stores_its_key_and_status` in
// apps/tui/src/app/mod.rs, run by the crate's own gate.

import { spawn } from 'node:child_process';
import { DatabaseSync } from 'node:sqlite';
import {
  chmodSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const BIN = join(ROOT, 'target', 'debug', 'task-timer-tui');
const DATE = '2026-09-17';
const KEY = 'US-1459';
const KEYED_LABEL = `${KEY} fix login`;
const PLAIN_LABEL = 'read the docs';
const SITE = 'https://stub.atlassian.test';
const EXPECTED_URL = `${SITE}/browse/${KEY}`;

// Mirrors the deployed schema closely enough for the app's reads.
const SCHEMA = `
CREATE TABLE users (id TEXT PRIMARY KEY, email TEXT NOT NULL UNIQUE, password_hash TEXT NOT NULL, created_at INTEGER NOT NULL);
CREATE TABLE tasks (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  user_id TEXT NOT NULL, label TEXT NOT NULL, description TEXT, code TEXT, link TEXT,
  status TEXT NOT NULL DEFAULT 'todo', notes TEXT, tags TEXT,
  elapsed_time INTEGER NOT NULL DEFAULT 0, total_time INTEGER NOT NULL DEFAULT 0,
  position INTEGER NOT NULL DEFAULT 0, is_running INTEGER NOT NULL DEFAULT 0,
  done INTEGER NOT NULL DEFAULT 0, is_completed INTEGER NOT NULL DEFAULT 0,
  is_cancelled INTEGER NOT NULL DEFAULT 0, is_deleted INTEGER NOT NULL DEFAULT 0,
  is_archived INTEGER NOT NULL DEFAULT 0, is_pinned INTEGER NOT NULL DEFAULT 0,
  is_important INTEGER NOT NULL DEFAULT 0, start_time INTEGER, end_time INTEGER,
  work_date TEXT NOT NULL, created_at INTEGER NOT NULL DEFAULT 0, updated_at INTEGER NOT NULL DEFAULT 0
);
CREATE UNIQUE INDEX idx_tasks_user_date_label ON tasks (user_id, work_date, label);
CREATE TABLE user_settings (user_id TEXT PRIMARY KEY, timer_mode TEXT NOT NULL DEFAULT 'focus', updated_at INTEGER NOT NULL DEFAULT 0);
CREATE TABLE task_comments (id INTEGER PRIMARY KEY AUTOINCREMENT, task_id INTEGER NOT NULL, subject TEXT, summary TEXT, branch TEXT, pr TEXT, created_at INTEGER NOT NULL);
CREATE TABLE task_integrations (id INTEGER PRIMARY KEY AUTOINCREMENT, task_id INTEGER NOT NULL, "group" TEXT NOT NULL, field TEXT NOT NULL, value TEXT, created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL);
`;

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

/** Strip escape sequences so the drawn frame can be matched as text. */
function plain(text) {
  return text
    .replace(/\x1b\[[0-9;?]*[a-zA-Z]/g, '\n')
    .replace(/\x1b[()][A-Z0-9]/g, '')
    .replace(/\x1b[<>=]/g, '')
    .replace(/\r/g, '\n');
}

/**
 * A fake `xdg-open` that records the URL it was handed. Written into a bin dir
 * prepended to PATH for the TUI only, so a real browser is never involved and
 * the assertion has an artifact to read.
 */
function makeOpener(dir, { present = true } = {}) {
  const binDir = join(dir, 'bin');
  mkdirSync(binDir, { recursive: true });
  const capture = join(dir, 'opener-argv.txt');
  const path = join(binDir, 'xdg-open');
  if (present) {
    writeFileSync(path, `#!/bin/sh\nprintf '%s\\n' "$1" >> ${JSON.stringify(capture)}\n`);
    chmodSync(path, 0o755);
  }
  return { binDir, capture };
}

/** One keyed task and one without a key, so both branches are observable. */
function makeFixture(dir) {
  const dbPath = join(dir, 'fixture.db');
  const configDir = join(dir, 'config');
  mkdirSync(configDir, { recursive: true });

  const email = `open-${Date.now()}@example.test`;
  const db = new DatabaseSync(dbPath);
  db.exec(SCHEMA);
  db.prepare('INSERT INTO users (id, email, password_hash, created_at) VALUES (?, ?, ?, 0)').run(
    email,
    email,
    'x'
  );
  const insert = db.prepare(
    'INSERT INTO tasks (user_id, label, description, work_date, position) VALUES (?, ?, ?, ?, ?)'
  );
  insert.run(email, KEYED_LABEL, 'see the ticket', DATE, 0);
  insert.run(email, PLAIN_LABEL, 'no ticket here', DATE, 1);
  db.close();

  writeFileSync(
    join(configDir, 'config.toml'),
    [`database_path = "${dbPath}"`, `user_email = "${email}"`, 'timer_mode = "focus"'].join('\n') +
      '\n'
  );
  return { dbPath, configDir };
}

/**
 * Run one TUI session in a pty, send `keys`, return the drawn frame as text.
 * `PATH` puts the fake opener first; `JIRA_SITE` pins the URL host.
 */
async function session(fixture, keys, env = {}) {
  const child = spawn(
    'script',
    [
      '-qec',
      `stty rows 45 cols 200; ${BIN} --config ${join(fixture.configDir, 'config.toml')} --date ${DATE}`,
      '/dev/null',
    ],
    {
      stdio: ['pipe', 'pipe', 'pipe'],
      env: { ...process.env, JIRA_SITE: SITE, ...env },
    }
  );
  let out = '';
  child.stdout.on('data', (c) => {
    out += c.toString('utf8');
  });
  child.stderr.on('data', (c) => {
    out += c.toString('utf8');
  });

  const deadline = Date.now() + 12_000;
  while (!plain(out).includes(KEYED_LABEL)) {
    if (Date.now() > deadline) {
      child.kill('SIGKILL');
      throw new Error('the TUI never drew its first frame in the pty');
    }
    await sleep(150);
  }

  for (const key of keys) {
    child.stdin.write(key);
    await sleep(800);
  }
  await sleep(400);
  const frame = plain(out);
  child.stdin.end();
  await sleep(200);
  child.kill('SIGKILL');
  return frame;
}

let failures = [];

function check(condition, message) {
  if (!condition) failures.push(message);
}

function assertToolchain() {
  if (!existsSync(BIN)) {
    failures.push(`${BIN} is missing — run: cargo build -p task-timer-tui`);
    return false;
  }
  if (process.platform !== 'linux') {
    failures.push('this check drives the TUI over a pty with `script`, which this platform lacks');
    return false;
  }
  return true;
}

/**
 * The handoff oracle, as a pure predicate so the control measures the same rule
 * rather than a lookalike: the opener must have received exactly the issue URL.
 */
function openerReceivedUrl(captured, expectedUrl) {
  return captured
    .split('\n')
    .map((line) => line.trim())
    .filter(Boolean)
    .includes(expectedUrl);
}

function capturedUrls(capturePath) {
  return existsSync(capturePath) ? readFileSync(capturePath, 'utf8') : '';
}

/**
 * G1: `o` on the keyed task hands its issue URL to the opener; the same key on
 * the unkeyed task below it must not reach the opener at all.
 */
async function openHolds() {
  const dir = mkdtempSync(join(tmpdir(), 'jira-open-'));
  try {
    const opener = makeOpener(dir);
    const fixture = makeFixture(dir);
    const env = { PATH: `${opener.binDir}:${process.env.PATH}` };

    // Row 0 is the keyed task: `o` must open its issue.
    const keyedFrame = await session(fixture, ['o'], env);
    check(
      openerReceivedUrl(capturedUrls(opener.capture), EXPECTED_URL),
      `the opener must receive ${EXPECTED_URL}, captured: ${JSON.stringify(capturedUrls(opener.capture))}`
    );
    check(
      /opened US-1459 in browser/.test(keyedFrame),
      `the status line must report the handoff:\n${keyedFrame.slice(-400)}`
    );

    // Move to row 1 (the keyless task) and press `o` again. The capture file
    // must not grow, and the status line must say why.
    const before = capturedUrls(opener.capture);
    const plainFrame = await session(fixture, ['j', 'o'], env);
    const after = capturedUrls(opener.capture);
    check(
      after === before,
      `a task with no JIRA key must not open anything, capture grew: ${JSON.stringify(after)}`
    );
    check(
      /no JIRA key/.test(plainFrame),
      `the keyless task must report the reason:\n${plainFrame.slice(-400)}`
    );

    return failures.length === 0;
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

/** G2: the oracle must reject a handoff that never happened. */
async function openControl() {
  const dir = mkdtempSync(join(tmpdir(), 'jira-open-control-'));
  try {
    // No `xdg-open` on PATH: the URL can never arrive, so the oracle must fail.
    const opener = makeOpener(dir, { present: false });
    const fixture = makeFixture(dir);
    await session(fixture, ['o'], { PATH: `${opener.binDir}:${process.env.PATH}` });

    if (openerReceivedUrl(capturedUrls(opener.capture), EXPECTED_URL)) {
      failures.push('the oracle accepted a handoff that never happened — it cannot fail');
      return false;
    }
    return true;
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

const selfTestMode = process.argv.includes('--self-test');
const passed = assertToolchain() && (selfTestMode ? await openControl() : await openHolds());

if (passed) {
  console.log(selfTestMode ? 'jira open control passed' : 'jira open verification passed');
  process.exit(0);
}
for (const failure of failures) console.error(`FAIL: ${failure}`);
process.exit(1);
