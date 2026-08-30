PRAGMA foreign_keys=OFF;--> statement-breakpoint
CREATE TABLE `__new_tasks` (
	`id` integer PRIMARY KEY AUTOINCREMENT NOT NULL,
	`user_id` text NOT NULL,
	`label` text NOT NULL,
	`work_date` text DEFAULT (date('now')) NOT NULL,
	`description` text,
	`elapsed_time` integer DEFAULT 0 NOT NULL,
	`position` integer DEFAULT 0 NOT NULL,
	`is_running` integer DEFAULT false NOT NULL,
	`done` integer DEFAULT false NOT NULL,
	`start_time` integer,
	`created_at` integer NOT NULL,
	`updated_at` integer NOT NULL,
	FOREIGN KEY (`user_id`) REFERENCES `users`(`id`) ON UPDATE no action ON DELETE cascade
);--> statement-breakpoint
INSERT INTO `__new_tasks` (`id`, `user_id`, `label`, `work_date`, `description`, `elapsed_time`, `position`, `is_running`, `done`, `start_time`, `created_at`, `updated_at`) SELECT `id`, `user_id`, `label`, date('now'), `description`, `elapsed_time`, `position`, `is_running`, `done`, `start_time`, `created_at`, `updated_at` FROM `tasks`;--> statement-breakpoint
DROP TABLE `tasks`;--> statement-breakpoint
ALTER TABLE `__new_tasks` RENAME TO `tasks`;--> statement-breakpoint
CREATE INDEX `idx_tasks_user` ON `tasks` (`user_id`);--> statement-breakpoint
CREATE INDEX `idx_tasks_position` ON `tasks` (`user_id`,`position`);--> statement-breakpoint
CREATE INDEX `idx_tasks_user_date` ON `tasks` (`user_id`,`work_date`);--> statement-breakpoint
CREATE UNIQUE INDEX `idx_tasks_user_date_label` ON `tasks` (`user_id`,`work_date`,`label`);--> statement-breakpoint
PRAGMA foreign_keys=ON;
