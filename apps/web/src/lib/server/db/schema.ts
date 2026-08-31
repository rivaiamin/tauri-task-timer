import { sql } from 'drizzle-orm';
import { sqliteTable, text, integer, index, uniqueIndex } from 'drizzle-orm/sqlite-core';

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
    workDate: text('work_date')
      .notNull()
      .default(sql`(date('now'))`),
    code: text('code'),
    description: text('description'),
    link: text('link'),
    status: text('status').notNull().default('todo'),
    notes: text('notes'),
    tags: text('tags', { mode: 'json' }).$type<string[]>(),
    elapsedTime: integer('elapsed_time').notNull().default(0),
    totalTime: integer('total_time').notNull().default(0),
    position: integer('position').notNull().default(0),
    isRunning: integer('is_running', { mode: 'boolean' }).notNull().default(false),
    done: integer('done', { mode: 'boolean' }).notNull().default(false),
    isCompleted: integer('is_completed', { mode: 'boolean' }).notNull().default(false),
    isCancelled: integer('is_cancelled', { mode: 'boolean' }).notNull().default(false),
    isDeleted: integer('is_deleted', { mode: 'boolean' }).notNull().default(false),
    isArchived: integer('is_archived', { mode: 'boolean' }).notNull().default(false),
    isPinned: integer('is_pinned', { mode: 'boolean' }).notNull().default(false),
    isImportant: integer('is_important', { mode: 'boolean' }).notNull().default(false),
    startTime: integer('start_time', { mode: 'timestamp_ms' }),
    endTime: integer('end_time', { mode: 'timestamp_ms' }),
    createdAt: integer('created_at', { mode: 'timestamp_ms' })
      .notNull()
      .$defaultFn(() => new Date()),
    updatedAt: integer('updated_at', { mode: 'timestamp_ms' })
      .notNull()
      .$defaultFn(() => new Date())
  },
  (t) => [
    index('idx_tasks_user').on(t.userId),
    index('idx_tasks_position').on(t.userId, t.position),
    index('idx_tasks_user_date').on(t.userId, t.workDate),
    uniqueIndex('idx_tasks_user_date_label').on(t.userId, t.workDate, t.label)
  ]
);

export const taskComments = sqliteTable(
  'task_comments',
  {
    id: integer('id').primaryKey({ autoIncrement: true }),
    taskId: integer('task_id')
      .notNull()
      .references(() => tasks.id, { onDelete: 'cascade' }),
    subject: text('subject'),
    summary: text('summary'),
    branch: text('branch'),
    pr: text('pr'),
    createdAt: integer('created_at', { mode: 'timestamp_ms' })
      .notNull()
      .$defaultFn(() => new Date())
  },
  (t) => [index('idx_task_comments_task').on(t.taskId)]
);

export const taskIntegrations = sqliteTable(
  'task_integrations',
  {
    id: integer('id').primaryKey({ autoIncrement: true }),
    taskId: integer('task_id')
      .notNull()
      .references(() => tasks.id, { onDelete: 'cascade' }),
    group: text('group').notNull(),
    field: text('field').notNull(),
    value: text('value'),
    createdAt: integer('created_at', { mode: 'timestamp_ms' })
      .notNull()
      .$defaultFn(() => new Date()),
    updatedAt: integer('updated_at', { mode: 'timestamp_ms' })
      .notNull()
      .$defaultFn(() => new Date())
  },
  (t) => [index('idx_task_integrations_task').on(t.taskId)]
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
export type TaskComment = typeof taskComments.$inferSelect;
export type TaskIntegration = typeof taskIntegrations.$inferSelect;
export type ApiKeyRow = typeof apiKeys.$inferSelect;
