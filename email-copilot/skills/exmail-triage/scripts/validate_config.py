#!/usr/bin/env python3
from __future__ import annotations

import argparse
import dataclasses
import json
import sys

from triage_lib import ConfigError, load_config


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Validate an Exmail triage YAML config file.")
    parser.add_argument("--config", required=True, help="Path to YAML config file.")
    parser.add_argument(
        "--show",
        action="store_true",
        help="Print the normalized config as JSON after validation.",
    )
    args = parser.parse_args(argv)

    try:
        config = load_config(args.config)
    except ConfigError as exc:
        print(f"Config error:\n{exc}", file=sys.stderr)
        return 1

    print("Config is valid.")
    if args.show:
        printable = {
            key: str(value) if hasattr(value, "__fspath__") else value
            for key, value in dataclasses.asdict(config).items()
        }
        printable["password_env_var"] = f"{config.password_env_var} (resolved)"
        printable["llm_api_key_env_var"] = f"{config.llm_api_key_env_var} (resolved)"
        print(json.dumps(printable, ensure_ascii=False, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
