// Verify the identity/day split against a copy of the real database.
//
// The migration's whole job is to turn one row per (user, work_date, label) into
// one identity per (user, label) plus one day row per worked day, WITHOUT losing
// recorded time. So this measures the before-state from the source DB itself and
// requires the after-state to match it exactly — the expected numbers are read
// from the source, never supplied by the caller.
//
//   node scripts/verify-day-identity.mjs --migration [--self-test]
//   node scripts/verify-day-identity.mjs --integrations
//
// --self-test is the negative control: it deletes the day rows before the
// predicate runs and requires the predicate to reject that.
//
// The service-layer behaviour (a task across two days, the archive's per-day
// breakdown) is asserted by `apps/web/src/lib/server/taskDayIdentity.test.ts`,
// which drives the real service; this script stays pure SQL.
import { copyFileSync, existsSync, mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createRequire } from 'node:module';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const WEB = join(ROOT, 'apps', 'web');
const require = createRequire(join(WEB, 'package.json'));

const Database = require('better-sqlite3');
const { drizzle } = require('drizzle-orm/better-sqlite3');
const { migrate } = require('drizzle-orm/better-sqlite3/migrator');

const args = process.argv.slice(2);
const has = (f) => args.includes(f);

/** A throwaway copy of the real DB, WAL and all. */
function copyRealDb() {
  const dir = mkdtempSync(join(tmpdir(), 'dayid-'));
  const dst = join(dir, 'copy.db');
  for (const suffix of ['', '-wal', '-shm']) {
    const src = join(WEB, 'local.db' + suffix);
    if (existsSync(src)) copyFileSync(src, dst + suffix);
  }
  return { dir, dst };
}

function runMigration(dbPath) {
  const sqlite = new Database(dbPath);
  sqlite.pragma('journal_mode = WAL');
  sqlite.pragma('foreign_keys = ON');
  migrate(drizzle(sqlite), { migrationsFolder: join(WEB, 'drizzle') });
  return sqlite;
}

/**
 * The before-state, measured from the source DB. `identities` is the number of
 * distinct (user_id, label) pairs, `days` the number of old rows, and `elapsed`
 * the summed recorded time — the three figures the migration must preserve.
 */
function measureSource(db) {
  const one = (sql) => db.prepare(sql).get();
  return {
    identities: one('SELECT COUNT(*) c FROM (SELECT DISTINCT user_id, label FROM tasks)').c,
    days: one('SELECT COUNT(*) c FROM tasks').c,
    elapsed: one('SELECT COALESCE(SUM(elapsed_time), 0) s FROM tasks').s,
  };
}

/** The after-state, measured from the migrated DB. */
function measureMigrated(db) {
  const one = (sql) => db.prepare(sql).get();
  return {
    identities: one('SELECT COUNT(*) c FROM tasks').c,
    days: one('SELECT COUNT(*) c FROM task_days').c,
    elapsed: one('SELECT COALESCE(SUM(elapsed_time), 0) s FROM task_days').s,
    orphanDays: one(
      'SELECT COUNT(*) c FROM task_days d LEFT JOIN tasks t ON t.id = d.task_id WHERE t.id IS NULL'
    ).c,
    duplicateDays: one(
      'SELECT COUNT(*) c FROM (SELECT task_id, work_date FROM task_days GROUP BY task_id, work_date HAVING COUNT(*) > 1)'
    ).c,
  };
}

/** Compare before/after and print the success marker only if everything holds. */
function assertPreserved(before, after, label) {
  const problems = [];
  if (after.identities !== before.identities) {
    problems.push(`identities ${after.identities} != ${before.identities}`);
  }
  if (after.days !== before.days) problems.push(`day rows ${after.days} != ${before.days}`);
  if (after.elapsed !== before.elapsed) {
    problems.push(`elapsed ${after.elapsed} != ${before.elapsed}`);
  }
  if (after.orphanDays !== 0) problems.push(`${after.orphanDays} orphan day rows`);
  if (after.duplicateDays !== 0) {
    problems.push(`${after.duplicateDays} duplicate (task, date) day rows`);
  }
  if (problems.length) {
    console.error(`${label}: ${problems.join('; ')}`);
    return false;
  }
  return true;
}

// ---------------------------------------------------------------------------
// --migration / --migration --self-test
// ---------------------------------------------------------------------------
if (has('--migration')) {
  const { dir, dst } = copyRealDb();
  try {
    const source = new Database(dst, { readonly: true });
    const before = measureSource(source);
    source.close();

    // Negative control: destroy the day rows, then require the predicate to fail.
    if (has('--self-test')) {
      const sqlite = runMigration(dst);
      sqlite.exec('DELETE FROM task_days');
      const after = measureMigrated(sqlite);
      sqlite.close();
      if (assertPreserved(before, after, 'control')) {
        console.error('control: a migration that dropped every day row was accepted');
        process.exit(1);
      }
      console.log(`migration control passed (rejected: ${after.days} of ${before.days} day rows)`);
      process.exit(0);
    }

    const sqlite = runMigration(dst);
    const after = measureMigrated(sqlite);
    sqlite.close();

    if (!assertPreserved(before, after, 'migration')) process.exit(1);
    console.log(
      `migration verification passed (${before.identities} identities, ${before.days} day rows, ${before.elapsed}s preserved)`
    );
    process.exit(0);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

// ---------------------------------------------------------------------------
// --integrations — task-level rows must not be duplicated per day.
// ---------------------------------------------------------------------------
if (has('--integrations')) {
  const { dir, dst } = copyRealDb();
  try {
    const sqlite = runMigration(dst);
    const dupes = sqlite
      .prepare(
        `SELECT COUNT(*) c FROM (
           SELECT task_id, "group", field FROM task_integrations
           GROUP BY task_id, "group", field HAVING COUNT(*) > 1)`
      )
      .get().c;
    const orphan = sqlite
      .prepare(
        `SELECT COUNT(*) c FROM task_integrations i
         LEFT JOIN tasks t ON t.id = i.task_id WHERE t.id IS NULL`
      )
      .get().c;
    const orphanComments = sqlite
      .prepare(
        `SELECT COUNT(*) c FROM task_comments c
         LEFT JOIN tasks t ON t.id = c.task_id WHERE t.id IS NULL`
      )
      .get().c;
    const total = sqlite.prepare('SELECT COUNT(*) c FROM task_integrations').get().c;
    sqlite.close();

    if (dupes !== 0 || orphan !== 0 || orphanComments !== 0) {
      console.error(
        `integrations: ${dupes} duplicated, ${orphan} orphaned, ${orphanComments} orphaned comments`
      );
      process.exit(1);
    }
    console.log(
      `integration verification passed (${total} task-level rows, none duplicated, none orphaned)`
    );
    process.exit(0);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

console.error('usage: verify-day-identity.mjs --migration [--self-test] | --integrations');
process.exit(2);
