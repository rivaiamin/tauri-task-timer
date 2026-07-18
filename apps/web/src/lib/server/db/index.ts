import Database from 'better-sqlite3';
import { drizzle } from 'drizzle-orm/better-sqlite3';
import { migrate } from 'drizzle-orm/better-sqlite3/migrator';
import { fileURLToPath } from 'node:url';
import { env } from '$env/dynamic/private';
import * as schema from './schema';

// Local, file-based SQLite. Configure the path with DATABASE_PATH (default local.db).
const dbPath = env.DATABASE_PATH || 'local.db';

const sqlite = new Database(dbPath);
sqlite.pragma('journal_mode = WAL');
sqlite.pragma('foreign_keys = ON');

export const db = drizzle(sqlite, { schema });

// Apply pending migrations on startup (idempotent). In dev the folder resolves
// from source; for production (adapter-node) run `pnpm db:migrate` at deploy.
try {
  const migrationsFolder = fileURLToPath(new URL('../../../../drizzle', import.meta.url));
  migrate(db, { migrationsFolder });
} catch (err) {
  console.error('[db] migration step skipped/failed:', err);
}

export { schema };
