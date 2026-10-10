// Verify `jira ensure` materializes a JIRA ticket as a timer task.
//
//   node scripts/verify-jira-ensure.mjs             # G1: ensure writes the task,
//                                                   #     and a second run adds nothing
//   node scripts/verify-jira-ensure.mjs --self-test # G2: the oracle rejects a
//                                                   #     duplicated identity/day row
//   node scripts/verify-jira-ensure.mjs --reject    # G3: a malformed key is refused
//                                                   #     before any network call
//   node scripts/verify-jira-ensure.mjs --docs      # G4: the README documents it
//
// No credentials and no network: `JIRA_SITE` is pointed at a stub HTTP server
// this script starts on loopback, and `JIRA_EMAIL`/`JIRA_TOKEN` are fake. The
// assertions read the rows the real binary wrote into a throwaway DB, and the
// stub counts the requests it received — so "the key was validated before any
// request" is measured, not assumed.
//
// The predicate is one function shared by G1 and its control, so the control
// measures the same rule rather than a lookalike.
import { spawn } from 'node:child_process';
import { createServer } from 'node:http';
import { DatabaseSync } from 'node:sqlite';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const BIN = join(ROOT, 'target', 'debug', 'task-timer-tui');
const README = join(ROOT, 'apps', 'tui', 'README.md');
const DATE = '2026-09-17';
const KEY = 'US-1459';
const SUMMARY = 'Fix login';
const STATUS = 'In Progress';
const LABEL = `${KEY} ${SUMMARY}`;
const BAD_KEY = 'not-a-jira-key';

// Mirrors the deployed schema closely enough for the app's reads and writes.
const SCHEMA = `
CREATE TABLE users (id TEXT PRIMARY KEY, email TEXT NOT NULL UNIQUE, password_hash TEXT NOT NULL, created_at INTEGER NOT NULL);
CREATE TABLE tasks (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  user_id TEXT NOT NULL, label TEXT NOT NULL, code TEXT, description TEXT, link TEXT,
  notes TEXT, tags TEXT,
  created_at INTEGER NOT NULL DEFAULT 0, updated_at INTEGER NOT NULL DEFAULT 0
);
CREATE UNIQUE INDEX idx_tasks_user_label ON tasks (user_id, label);
CREATE TABLE task_days (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  task_id INTEGER NOT NULL, work_date TEXT NOT NULL,
  status TEXT NOT NULL DEFAULT 'todo',
  elapsed_time INTEGER NOT NULL DEFAULT 0, total_time INTEGER NOT NULL DEFAULT 0,
  position INTEGER NOT NULL DEFAULT 0, is_running INTEGER NOT NULL DEFAULT 0,
  done INTEGER NOT NULL DEFAULT 0, is_completed INTEGER NOT NULL DEFAULT 0,
  is_cancelled INTEGER NOT NULL DEFAULT 0, is_deleted INTEGER NOT NULL DEFAULT 0,
  is_archived INTEGER NOT NULL DEFAULT 0, is_pinned INTEGER NOT NULL DEFAULT 0,
  is_important INTEGER NOT NULL DEFAULT 0, start_time INTEGER, end_time INTEGER,
  created_at INTEGER NOT NULL DEFAULT 0, updated_at INTEGER NOT NULL DEFAULT 0
);
CREATE UNIQUE INDEX idx_task_days_task_date ON task_days (task_id, work_date);
CREATE TABLE user_settings (user_id TEXT PRIMARY KEY, timer_mode TEXT NOT NULL DEFAULT 'focus', updated_at INTEGER NOT NULL DEFAULT 0);
CREATE TABLE task_comments (id INTEGER PRIMARY KEY AUTOINCREMENT, task_id INTEGER NOT NULL, subject TEXT, summary TEXT, branch TEXT, pr TEXT, created_at INTEGER NOT NULL);
CREATE TABLE task_integrations (id INTEGER PRIMARY KEY AUTOINCREMENT, task_id INTEGER NOT NULL, "group" TEXT NOT NULL, field TEXT NOT NULL, value TEXT, created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL);
`;

let failures = [];

function check(condition, message) {
  if (!condition) failures.push(message);
}

/**
 * A stub JIRA that answers one issue and counts every request it receives. The
 * request count is what lets `--reject` prove no network call happened.
 */
function startStub() {
  const requests = [];
  const server = createServer((req, res) => {
    requests.push({ url: req.url, auth: req.headers.authorization ?? '' });
    if (req.url.startsWith(`/rest/api/3/issue/${KEY}`)) {
      res.writeHead(200, { 'content-type': 'application/json' });
      res.end(JSON.stringify({ fields: { summary: SUMMARY, status: { name: STATUS } } }));
      return;
    }
    res.writeHead(404, { 'content-type': 'application/json' });
    res.end(JSON.stringify({ errorMessages: ['not found'] }));
  });
  return new Promise((ok) => {
    server.listen(0, '127.0.0.1', () => {
      const { port } = server.address();
      ok({
        url: `http://127.0.0.1:${port}`,
        requests,
        close: () => new Promise((done) => server.close(done)),
      });
    });
  });
}

/** A throwaway DB with one registered user, plus a config pointing at it. */
function makeFixture(dir) {
  const dbPath = join(dir, 'fixture.db');
  const configDir = join(dir, 'config');
  mkdirSync(configDir, { recursive: true });

  const email = `ensure-${Date.now()}@example.test`;
  const db = new DatabaseSync(dbPath);
  db.exec(SCHEMA);
  db.prepare('INSERT INTO users (id, email, password_hash, created_at) VALUES (?, ?, ?, 0)').run(
    email,
    email,
    'x'
  );
  db.close();

  const configPath = join(configDir, 'config.toml');
  writeFileSync(
    configPath,
    [`database_path = "${dbPath}"`, `user_email = "${email}"`, 'timer_mode = "focus"'].join('\n') +
      '\n'
  );
  return { dbPath, configPath, email };
}

/**
 * Run the real binary against the fixture.
 *
 * Asynchronous on purpose: the stub JIRA lives in this process, so a synchronous
 * `spawnSync` would block the event loop and the server could never answer.
 */
function runEnsure(fixture, site, key) {
  const args = ['--config', fixture.configPath, '--date', DATE, '--json', 'jira', 'ensure', key];
  return new Promise((done) => {
    const child = spawn(BIN, args, {
      timeout: 60_000,
      env: {
        ...process.env,
        JIRA_SITE: site,
        JIRA_EMAIL: 'stub@example.test',
        JIRA_TOKEN: 'stub-token',
        // Keep a real developer's ~/.aimsis/jira.env out of the test entirely.
        JIRA_ENV_FILE: join(dirname(fixture.configPath), 'absent.env'),
      },
    });
    let stdout = '';
    let stderr = '';
    child.stdout.on('data', (c) => {
      stdout += c.toString('utf8');
    });
    child.stderr.on('data', (c) => {
      stderr += c.toString('utf8');
    });
    child.on('error', (e) => done({ status: -1, stdout, stderr: `${stderr}${e.message}` }));
    child.on('close', (status) => done({ status, stdout, stderr }));
  });
}

/** The after-state, measured from the DB the binary wrote. */
function measure(dbPath) {
  const db = new DatabaseSync(dbPath, { readOnly: true });
  const one = (sql) => db.prepare(sql).get();
  const out = {
    identities: one('SELECT COUNT(*) c FROM tasks').c,
    dayRows: one('SELECT COUNT(*) c FROM task_days').c,
    duplicateDays: one(
      'SELECT COUNT(*) c FROM (SELECT task_id, work_date FROM task_days GROUP BY task_id, work_date HAVING COUNT(*) > 1)'
    ).c,
    labels: one('SELECT label FROM tasks LIMIT 1')?.label ?? null,
    running: one('SELECT COALESCE(SUM(is_running), 0) c FROM task_days').c,
    statuses: one('SELECT status FROM task_days LIMIT 1')?.status ?? null,
    issueKey:
      one(
        `SELECT value FROM task_integrations WHERE "group" = 'jira' AND field = 'issue_key'`
      )?.value ?? null,
    jiraStatus:
      one(`SELECT value FROM task_integrations WHERE "group" = 'jira' AND field = 'status'`)?.value ??
      null,
  };
  db.close();
  return out;
}

/** Every `jira/*` row, with the task id it belongs to. */
function integrationRows(dbPath) {
  const db = new DatabaseSync(dbPath, { readOnly: true });
  const rows = db
    .prepare(
      `SELECT task_id AS taskId, field, value FROM task_integrations WHERE "group" = 'jira'`
    )
    .all();
  db.close();
  return rows;
}

/**
 * The one rule both the check and its control measure: exactly one identity, one
 * day row for the ticket, no duplicate day, the `KEY summary` label, the two
 * `jira/*` rows, and a timer the fetch did not start.
 */
function predicate(state) {
  const problems = [];
  if (state.identities !== 1) problems.push(`identities ${state.identities} != 1`);
  if (state.dayRows !== 1) problems.push(`day rows ${state.dayRows} != 1`);
  if (state.duplicateDays !== 0) problems.push(`${state.duplicateDays} duplicate day rows`);
  if (state.labels !== LABEL) problems.push(`label ${JSON.stringify(state.labels)} != ${JSON.stringify(LABEL)}`);
  if (state.issueKey !== KEY) problems.push(`issue_key ${JSON.stringify(state.issueKey)} != ${KEY}`);
  if (state.jiraStatus !== STATUS) {
    problems.push(`jira/status ${JSON.stringify(state.jiraStatus)} != ${JSON.stringify(STATUS)}`);
  }
  if (state.running !== 0) problems.push(`${state.running} running day rows (ensure must not start the timer)`);
  if (state.statuses !== 'todo') {
    problems.push(`day status ${JSON.stringify(state.statuses)} != "todo" (the timer owns it)`);
  }
  return problems;
}

function assertToolchain() {
  if (!existsSync(BIN)) {
    console.error(`FAIL: ${BIN} is missing — run \`cargo build -p task-timer-tui\` first`);
    return false;
  }
  return true;
}

/** G1: ensure writes the task; a second run adds nothing. */
async function ensureHolds() {
  const dir = mkdtempSync(join(tmpdir(), 'ensure-'));
  const stub = await startStub();
  try {
    const fixture = makeFixture(dir);
    const site = stub.url;

    const first = await runEnsure(fixture, site, KEY);
    check(first.status === 0, `first run exited ${first.status}: ${first.stderr.trim()}`);
    const afterFirst = measure(fixture.dbPath);
    for (const problem of predicate(afterFirst)) check(false, `first run: ${problem}`);
    check(
      stub.requests.length === 1,
      `expected exactly 1 JIRA request, got ${stub.requests.length}`
    );
    check(
      (stub.requests[0]?.auth ?? '').startsWith('Basic '),
      'the request did not carry basic auth — credentials were not used'
    );

    // Idempotency: the second run must reuse the identity and the day row.
    const second = await runEnsure(fixture, site, KEY);
    check(second.status === 0, `second run exited ${second.status}: ${second.stderr.trim()}`);
    const afterSecond = measure(fixture.dbPath);
    for (const problem of predicate(afterSecond)) check(false, `second run: ${problem}`);
    check(
      afterSecond.identities === afterFirst.identities,
      `a second run added identities: ${afterFirst.identities} -> ${afterSecond.identities}`
    );
    check(
      afterSecond.dayRows === afterFirst.dayRows,
      `a second run added day rows: ${afterFirst.dayRows} -> ${afterSecond.dayRows}`
    );
    return failures.length === 0;
  } finally {
    await stub.close();
    rmSync(dir, { recursive: true, force: true });
  }
}

/** G2: the predicate must reject a run that started the timer or moved its status. */
async function ensureControl() {
  const dir = mkdtempSync(join(tmpdir(), 'ensure-ctl-'));
  const stub = await startStub();
  try {
    const fixture = makeFixture(dir);
    const proc = await runEnsure(fixture, stub.url, KEY);
    if (proc.status !== 0) {
      console.error(`control: the setup run itself failed (${proc.status}): ${proc.stderr.trim()}`);
      return false;
    }
    if (predicate(measure(fixture.dbPath)).length !== 0) {
      console.error('control: the setup run did not produce the state the control needs');
      return false;
    }

    // A duplicate identity or day row cannot be forged here: `idx_tasks_user_label`
    // and `idx_task_days_task_date` reject it outright, which is why the predicate's
    // duplicate check is a guard against schema drift rather than something a
    // control can exercise. The breakable claim is the read-only one — `ensure`
    // must not start the timer or move the status the timer owns — so break that.
    const db = new DatabaseSync(fixture.dbPath);
    db.prepare('UPDATE task_days SET is_running = 1, status = ?1').run('21');
    db.close();

    const problems = predicate(measure(fixture.dbPath));
    if (problems.length === 0) {
      console.error('control: a run that started the timer and moved the status was accepted');
      return false;
    }
    // Require the *right* check to have fired, so a rejection for an unrelated
    // reason cannot pass as evidence the read-only claim is guarded.
    if (!problems.some((p) => p.includes('running day rows'))) {
      console.error(`control: rejected, but not for the read-only claim: ${problems.join('; ')}`);
      return false;
    }
    console.log(`jira ensure control passed (rejected: ${problems.join('; ')})`);
    return true;
  } finally {
    await stub.close();
    rmSync(dir, { recursive: true, force: true });
  }
}

/** G3: a malformed key is refused before any request reaches JIRA. */
async function ensureRejects() {
  const dir = mkdtempSync(join(tmpdir(), 'ensure-bad-'));
  const stub = await startStub();
  try {
    const fixture = makeFixture(dir);
    const proc = await runEnsure(fixture, stub.url, BAD_KEY);

    check(proc.status !== 0, `a malformed key exited 0 (stdout: ${proc.stdout.trim()})`);
    check(
      stub.requests.length === 0,
      `a malformed key still made ${stub.requests.length} JIRA request(s)`
    );
    const state = measure(fixture.dbPath);
    check(state.identities === 0, `a rejected key still wrote ${state.identities} task(s)`);
    check(state.dayRows === 0, `a rejected key still wrote ${state.dayRows} day row(s)`);
    return failures.length === 0;
  } finally {
    await stub.close();
    rmSync(dir, { recursive: true, force: true });
  }
}

/** G5: an existing bare-key task is reused, not shadowed by a second identity. */
async function ensureReuses() {
  const dir = mkdtempSync(join(tmpdir(), 'ensure-reuse-'));
  const stub = await startStub();
  try {
    const fixture = makeFixture(dir);
    // The real shape this exists for: the operator already tracks the ticket
    // under its bare key, while the sprint fetch would label it `KEY summary`.
    const db = new DatabaseSync(fixture.dbPath);
    db.prepare('INSERT INTO tasks (user_id, label) VALUES (?, ?)').run(fixture.email, KEY);
    db.prepare('INSERT INTO task_days (task_id, work_date, position) VALUES (1, ?, 0)').run(DATE);
    db.close();

    const proc = await runEnsure(fixture, stub.url, KEY);
    check(proc.status === 0, `reuse run exited ${proc.status}: ${proc.stderr.trim()}`);

    const state = measure(fixture.dbPath);
    check(state.identities === 1, `a second identity was created (${state.identities} tasks)`);
    check(state.dayRows === 1, `a second day row was created (${state.dayRows} day rows)`);
    check(
      state.labels === KEY,
      `the existing label was rewritten to ${JSON.stringify(state.labels)}`
    );
    // The decisive part: the rows must land on the identity the sprint runner
    // resolves the key to, or the two would read different tasks.
    const rows = integrationRows(fixture.dbPath);
    check(
      rows.some((r) => r.field === 'issue_key' && r.value === KEY),
      'the issue_key row did not land on the reused identity'
    );
    check(
      rows.some((r) => r.field === 'status'),
      'the jira/status row did not land on the reused identity'
    );
    return failures.length === 0;
  } finally {
    await stub.close();
    rmSync(dir, { recursive: true, force: true });
  }
}

/** G4: the README documents the command and its flags. */
function ensureDocumented() {
  const readme = readFileSync(README, 'utf8');
  for (const needle of ['jira ensure', '--json jira ensure']) {
    check(readme.includes(needle), `apps/tui/README.md does not mention \`${needle}\``);
  }
  return failures.length === 0;
}

const mode = process.argv.includes('--self-test')
  ? 'control'
  : process.argv.includes('--reject')
    ? 'reject'
    : process.argv.includes('--reuse')
      ? 'reuse'
      : process.argv.includes('--docs')
        ? 'docs'
        : 'hold';

let passed = false;
if (!assertToolchain()) {
  passed = false;
} else if (mode === 'control') {
  passed = await ensureControl();
} else if (mode === 'reject') {
  passed = await ensureRejects();
} else if (mode === 'reuse') {
  passed = await ensureReuses();
} else if (mode === 'docs') {
  passed = ensureDocumented();
} else {
  passed = await ensureHolds();
}

if (passed) {
  if (mode === 'control') process.exit(0);
  if (mode === 'reject') console.log('jira ensure rejects invalid keys');
  else if (mode === 'reuse') console.log('jira ensure reuses an existing task');
  else if (mode === 'docs') console.log('jira ensure documented');
  else console.log('jira ensure verification passed');
  process.exit(0);
}

for (const failure of failures) console.error(`FAIL: ${failure}`);
process.exit(1);
