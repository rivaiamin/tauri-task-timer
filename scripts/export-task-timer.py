#!/usr/bin/env python3
"""Export task-timer configuration and a consistent SQLite database snapshot."""

import argparse
import os
import shutil
import sqlite3
import sys
import tomllib
from datetime import datetime
from pathlib import Path


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--config",
        type=Path,
        default=Path.home() / ".config/task-timer-tui/config.toml",
        help="task-timer config file (default: ~/.config/task-timer-tui/config.toml)",
    )
    parser.add_argument(
        "--output-dir",
        type=Path,
        help="new export directory (default: ~/task-timer-exports/task-timer-export-TIMESTAMP)",
    )
    args = parser.parse_args()

    config_path = args.config.expanduser().resolve()
    if not config_path.is_file():
        parser.error(f"config file not found: {config_path}")

    try:
        config = tomllib.loads(config_path.read_text(encoding="utf-8"))
    except (OSError, tomllib.TOMLDecodeError) as error:
        parser.error(f"cannot read config {config_path}: {error}")

    database_value = config.get("database_path")
    if not isinstance(database_value, str) or not database_value.strip():
        parser.error(f"config must contain a non-empty database_path: {config_path}")
    database_path = Path(database_value).expanduser()
    if not database_path.is_absolute():
        database_path = Path.cwd() / database_path
    database_path = database_path.resolve()
    if not database_path.is_file():
        parser.error(f"database file not found: {database_path}")

    timestamp = datetime.now().astimezone().strftime("%Y%m%d-%H%M%S")
    output_dir = args.output_dir or (
        Path.home() / "task-timer-exports" / f"task-timer-export-{timestamp}"
    )
    output_dir = output_dir.expanduser().resolve()
    try:
        output_dir.mkdir(parents=True, mode=0o700, exist_ok=False)
    except FileExistsError:
        parser.error(f"export directory already exists: {output_dir}")
    os.chmod(output_dir, 0o700)

    config_export = output_dir / "config.toml"
    database_export = output_dir / database_path.name
    try:
        shutil.copy2(config_path, config_export)
        os.chmod(config_export, 0o600)
        with sqlite3.connect(database_path) as source, sqlite3.connect(database_export) as target:
            source.backup(target)
            result = target.execute("PRAGMA quick_check").fetchone()
            if result != ("ok",):
                raise sqlite3.DatabaseError(f"database quick_check failed: {result}")
        os.chmod(database_export, 0o600)
    except Exception:
        shutil.rmtree(output_dir)
        raise

    print(f"Exported config and database to {output_dir}")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, sqlite3.Error) as error:
        print(f"export failed: {error}", file=sys.stderr)
        raise SystemExit(1)
