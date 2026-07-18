import { sqliteTable, text, integer, index } from 'drizzle-orm/sqlite-core';

// Users (replaces Supabase auth.users). Passwords stored as argon2 hashes.
export const users = sqliteTable('users', {
  id: text('id').primaryKey(), // app-generated uuid
  email: text('email').notNull().unique(),
  passwordHash: text('password_hash').notNull(),
  createdAt: integer('created_at', { mode: 'timestamp_ms' })
    .notNull()
    .$defaultFn(() => new Date())
});

// Session tokens. `id` is the SHA-256 hash of the raw token; the raw token
// lives only in the user's httpOnly cookie.
export const sessions = sqliteTable('sessions', {
  id: text('id').primaryKey(),
  userId: text('user_id')
    .notNull()
    .references(() => users.id, { onDelete: 'cascade' }),
  expiresAt: integer('expires_at', { mode: 'timestamp_ms' }).notNull()
});

export const tasks = sqliteTable(
  'tasks',
  {
    id: integer('id').primaryKey({ autoIncrement: true }),
    userId: text('user_id')
      .notNull()
      .references(() => users.id, { onDelete: 'cascade' }),
    label: text('label').notNull(),
    description: text('description'),
    elapsedTime: integer('elapsed_time').notNull().default(0),
    position: integer('position').notNull().default(0),
    isRunning: integer('is_running', { mode: 'boolean' }).notNull().default(false),
    startTime: integer('start_time', { mode: 'timestamp_ms' }),
    createdAt: integer('created_at', { mode: 'timestamp_ms' })
      .notNull()
      .$defaultFn(() => new Date()),
    updatedAt: integer('updated_at', { mode: 'timestamp_ms' })
      .notNull()
      .$defaultFn(() => new Date())
  },
  (t) => [index('idx_tasks_user').on(t.userId), index('idx_tasks_position').on(t.userId, t.position)]
);

export const userSettings = sqliteTable('user_settings', {
  userId: text('user_id')
    .primaryKey()
    .references(() => users.id, { onDelete: 'cascade' }),
  timerMode: text('timer_mode').notNull().default('focus'), // 'focus' | 'parallel'
  updatedAt: integer('updated_at', { mode: 'timestamp_ms' })
    .notNull()
    .$defaultFn(() => new Date())
});

// API keys for programmatic / AI-agent access. Only the SHA-256 hash is stored.
export const apiKeys = sqliteTable('api_keys', {
  id: text('id').primaryKey(),
  userId: text('user_id')
    .notNull()
    .references(() => users.id, { onDelete: 'cascade' }),
  keyHash: text('key_hash').notNull().unique(),
  keyPrefix: text('key_prefix').notNull(),
  name: text('name'),
  scopes: text('scopes', { mode: 'json' })
    .$type<string[]>()
    .notNull()
    .$defaultFn(() => ['tasks:read', 'tasks:write']),
  revoked: integer('revoked', { mode: 'boolean' }).notNull().default(false),
  lastUsedAt: integer('last_used_at', { mode: 'timestamp_ms' }),
  createdAt: integer('created_at', { mode: 'timestamp_ms' })
    .notNull()
    .$defaultFn(() => new Date())
});

export type User = typeof users.$inferSelect;
export type Session = typeof sessions.$inferSelect;
export type Task = typeof tasks.$inferSelect;
export type ApiKeyRow = typeof apiKeys.$inferSelect;
