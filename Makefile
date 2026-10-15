.PHONY: export-task-timer

TASK_TIMER_CONFIG ?= $(HOME)/.config/task-timer-tui/config.toml
EXPORT_DIR ?=

export-task-timer:
	python3 scripts/export-task-timer.py --config "$(TASK_TIMER_CONFIG)" $(if $(EXPORT_DIR),--output-dir "$(EXPORT_DIR)")
