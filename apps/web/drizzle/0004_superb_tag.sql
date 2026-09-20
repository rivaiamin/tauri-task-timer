-- Split the day-scoped `tasks` row into a task identity plus one row per day.
--
-- `PRAGMA foreign_keys=OFF` cannot be used here: the migrator runs migrations
-- inside a transaction, where the pragma is a no-op, and dropping `tasks` with
-- foreign keys on would cascade the freshly copied children away. Instead the
-- child tables are dropped before `tasks`, and rebuilt afterwards, so no
-- cascade is ever in flight.
CREATE TABLE `__new_tasks` (
	`id` integer PRIMARY KEY AUTOINCREMENT NOT NULL,
	`user_id` text NOT NULL,
	`label` text NOT NULL,
	`code` text,
	`description` text,
	`link` text,
	`notes` text,
	`tags` text,
	`created_at` integer NOT NULL,
	`updated_at` integer NOT NULL,
	FOREIGN KEY (`user_id`) REFERENCES `users`(`id`) ON UPDATE no action ON DELETE cascade
);--> statement-breakpoint
CREATE TABLE `__tmp_tasks` AS SELECT * FROM `tasks`;--> statement-breakpoint
CREATE TABLE `__tmp_comments` AS SELECT * FROM `task_comments`;--> statement-breakpoint
CREATE TABLE `__tmp_integrations` AS SELECT * FROM `task_integrations`;--> statement-breakpoint
-- One identity row per (user_id, label). The identity id is the group's smallest
-- id so existing references stay stable; the content fields come from the
-- group's most recent day row, which is what "copy the latest description"
-- already did at runtime.
INSERT INTO `__new_tasks` (`id`, `user_id`, `label`, `code`, `description`, `link`, `notes`, `tags`, `created_at`, `updated_at`)
SELECT
	MIN(t.`id`),
	t.`user_id`,
	t.`label`,
	(SELECT x.`code`        FROM `__tmp_tasks` x WHERE x.`user_id` = t.`user_id` AND x.`label` = t.`label` ORDER BY x.`work_date` DESC, x.`id` DESC LIMIT 1),
	(SELECT x.`description` FROM `__tmp_tasks` x WHERE x.`user_id` = t.`user_id` AND x.`label` = t.`label` ORDER BY x.`work_date` DESC, x.`id` DESC LIMIT 1),
	(SELECT x.`link`        FROM `__tmp_tasks` x WHERE x.`user_id` = t.`user_id` AND x.`label` = t.`label` ORDER BY x.`work_date` DESC, x.`id` DESC LIMIT 1),
	(SELECT x.`notes`       FROM `__tmp_tasks` x WHERE x.`user_id` = t.`user_id` AND x.`label` = t.`label` ORDER BY x.`work_date` DESC, x.`id` DESC LIMIT 1),
	(SELECT x.`tags`        FROM `__tmp_tasks` x WHERE x.`user_id` = t.`user_id` AND x.`label` = t.`label` ORDER BY x.`work_date` DESC, x.`id` DESC LIMIT 1),
	MIN(t.`created_at`),
	MAX(t.`updated_at`)
FROM `__tmp_tasks` t
GROUP BY t.`user_id`, t.`label`;--> statement-breakpoint
DROP TABLE `task_comments`;--> statement-breakpoint
DROP TABLE `task_integrations`;--> statement-breakpoint
DROP TABLE `tasks`;--> statement-breakpoint
ALTER TABLE `__new_tasks` RENAME TO `tasks`;--> statement-breakpoint
CREATE TABLE `task_days` (
	`id` integer PRIMARY KEY AUTOINCREMENT NOT NULL,
	`task_id` integer NOT NULL,
	`work_date` text DEFAULT (date('now')) NOT NULL,
	`status` text DEFAULT 'todo' NOT NULL,
	`elapsed_time` integer DEFAULT 0 NOT NULL,
	`total_time` integer DEFAULT 0 NOT NULL,
	`position` integer DEFAULT 0 NOT NULL,
	`is_running` integer DEFAULT false NOT NULL,
	`done` integer DEFAULT false NOT NULL,
	`is_completed` integer DEFAULT false NOT NULL,
	`is_cancelled` integer DEFAULT false NOT NULL,
	`is_deleted` integer DEFAULT false NOT NULL,
	`is_archived` integer DEFAULT false NOT NULL,
	`is_pinned` integer DEFAULT false NOT NULL,
	`is_important` integer DEFAULT false NOT NULL,
	`start_time` integer,
	`end_time` integer,
	`created_at` integer NOT NULL,
	`updated_at` integer NOT NULL,
	FOREIGN KEY (`task_id`) REFERENCES `tasks`(`id`) ON UPDATE no action ON DELETE cascade
);--> statement-breakpoint
-- Every old row becomes one day of its task, so no recorded time is lost.
INSERT INTO `task_days` (`task_id`, `work_date`, `status`, `elapsed_time`, `total_time`, `position`, `is_running`, `done`, `is_completed`, `is_cancelled`, `is_deleted`, `is_archived`, `is_pinned`, `is_important`, `start_time`, `end_time`, `created_at`, `updated_at`)
SELECT
	(SELECT MIN(m.`id`) FROM `__tmp_tasks` m WHERE m.`user_id` = t.`user_id` AND m.`label` = t.`label`),
	t.`work_date`, t.`status`, t.`elapsed_time`, t.`total_time`, t.`position`,
	t.`is_running`, t.`done`, t.`is_completed`, t.`is_cancelled`, t.`is_deleted`,
	t.`is_archived`, t.`is_pinned`, t.`is_important`,
	t.`start_time`, t.`end_time`, t.`created_at`, t.`updated_at`
FROM `__tmp_tasks` t;--> statement-breakpoint
CREATE TABLE `task_comments` (
	`id` integer PRIMARY KEY AUTOINCREMENT NOT NULL,
	`task_id` integer NOT NULL,
	`subject` text,
	`summary` text,
	`branch` text,
	`pr` text,
	`created_at` integer NOT NULL,
	FOREIGN KEY (`task_id`) REFERENCES `tasks`(`id`) ON UPDATE no action ON DELETE cascade
);--> statement-breakpoint
-- Comments are task-level, so each day row's copy collapses onto the surviving
-- identity and the duplicates go.
INSERT INTO `task_comments` (`task_id`, `subject`, `summary`, `branch`, `pr`, `created_at`)
SELECT
	(SELECT MIN(m.`id`) FROM `__tmp_tasks` m WHERE m.`id` = c.`task_id`),
	c.`subject`, c.`summary`, c.`branch`, c.`pr`, c.`created_at`
FROM `__tmp_comments` c
WHERE c.`id` IN (
	SELECT MIN(c2.`id`) FROM `__tmp_comments` c2
	GROUP BY (SELECT MIN(m.`id`) FROM `__tmp_tasks` m WHERE m.`id` = c2.`task_id`),
		COALESCE(c2.`subject`, ''), COALESCE(c2.`summary`, ''), COALESCE(c2.`branch`, ''), COALESCE(c2.`pr`, '')
);--> statement-breakpoint
CREATE TABLE `task_integrations` (
	`id` integer PRIMARY KEY AUTOINCREMENT NOT NULL,
	`task_id` integer NOT NULL,
	`group` text NOT NULL,
	`field` text NOT NULL,
	`value` text,
	`created_at` integer NOT NULL,
	`updated_at` integer NOT NULL,
	FOREIGN KEY (`task_id`) REFERENCES `tasks`(`id`) ON UPDATE no action ON DELETE cascade
);--> statement-breakpoint
-- Same for integrations: the JIRA key and its status belong to the task, not to
-- each day it was worked.
INSERT INTO `task_integrations` (`task_id`, `group`, `field`, `value`, `created_at`, `updated_at`)
SELECT
	(SELECT MIN(m.`id`) FROM `__tmp_tasks` m WHERE m.`id` = i.`task_id`),
	i.`group`, i.`field`, i.`value`, i.`created_at`, i.`updated_at`
FROM `__tmp_integrations` i
WHERE i.`id` IN (
	SELECT MIN(i2.`id`) FROM `__tmp_integrations` i2
	GROUP BY (SELECT MIN(m.`id`) FROM `__tmp_tasks` m WHERE m.`id` = i2.`task_id`), i2.`group`, i2.`field`
);--> statement-breakpoint
DROP TABLE `__tmp_tasks`;--> statement-breakpoint
DROP TABLE `__tmp_comments`;--> statement-breakpoint
DROP TABLE `__tmp_integrations`;--> statement-breakpoint
CREATE INDEX `idx_tasks_user` ON `tasks` (`user_id`);--> statement-breakpoint
CREATE UNIQUE INDEX `idx_tasks_user_label` ON `tasks` (`user_id`,`label`);--> statement-breakpoint
CREATE INDEX `idx_task_days_task` ON `task_days` (`task_id`);--> statement-breakpoint
CREATE INDEX `idx_task_days_date_position` ON `task_days` (`work_date`,`position`);--> statement-breakpoint
CREATE INDEX `idx_task_days_date` ON `task_days` (`work_date`);--> statement-breakpoint
CREATE UNIQUE INDEX `idx_task_days_task_date` ON `task_days` (`task_id`,`work_date`);--> statement-breakpoint
CREATE INDEX `idx_task_comments_task` ON `task_comments` (`task_id`);--> statement-breakpoint
CREATE INDEX `idx_task_integrations_task` ON `task_integrations` (`task_id`);
