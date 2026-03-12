#!/usr/bin/env python3
from __future__ import annotations

import argparse
import sys

from triage_lib import ConfigError, connect_imap, load_config, select_mailbox


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description="Check Exmail IMAP login and INBOX access without running digest or LLM steps."
    )
    parser.add_argument("--config", required=True, help="Path to YAML config file.")
    args = parser.parse_args(argv)

    try:
        config = load_config(args.config)
    except ConfigError as exc:
        print(f"Config error:\n{exc}", file=sys.stderr)
        return 1

    client = None
    try:
        client = connect_imap(config)
        uidvalidity = select_mailbox(client, "INBOX")
        print("IMAP login check passed.")
        print(f"Mailbox: INBOX")
        print(f"UIDVALIDITY: {uidvalidity}")
        return 0
    except Exception as exc:
        print(f"IMAP login check failed: {exc}", file=sys.stderr)
        return 1
    finally:
        if client is not None:
            try:
                client.logout()
            except Exception:
                pass


if __name__ == "__main__":
    raise SystemExit(main())
