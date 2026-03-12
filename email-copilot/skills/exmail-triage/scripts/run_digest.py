#!/usr/bin/env python3
from __future__ import annotations

import datetime as dt
import sys

from triage_lib import (
    ConfigError,
    ensure_output_paths,
    filter_recent_records,
    has_processed,
    initialize_state_db,
    list_candidate_records,
    load_config,
    parse_args,
    print_json,
    record_processed,
    select_mailbox,
    summarize_run,
    connect_imap,
    write_reports,
)


def main(argv: list[str] | None = None) -> int:
    """执行一次邮件摘要流程，支持 dry-run 和正式落盘两种模式。"""
    args = parse_args(argv)
    try:
        config = load_config(args.config)
        if not args.dry_run:
            # 正式运行会写报告、归档和状态库，网络调用前先确保目录已经准备好。
            ensure_output_paths(config)
    except ConfigError as exc:
        print(f"Config error:\n{exc}", file=sys.stderr)
        return 1

    limit = args.limit or config.max_emails_per_run
    connection = None
    client = None
    try:
        # dry-run 必须保持无副作用，因此不初始化状态库，也不写任何文件。
        connection = initialize_state_db(config.state_db_path) if not args.dry_run else None
        client = connect_imap(config)
        mailbox_name = "INBOX"
        uidvalidity = select_mailbox(client, mailbox_name)
        records = list_candidate_records(
            client,
            mailbox=mailbox_name,
            uidvalidity=uidvalidity,
            timezone_name=config.timezone,
            limit=limit,
        )
        if connection is not None:
            # 用 UIDVALIDITY + UID 做幂等去重，避免定时任务重复处理同一封邮件。
            records = [
                record
                for record in records
                if not has_processed(connection, mailbox_name, uidvalidity, record.uid)
            ]
        records = filter_recent_records(records, since_minutes=args.since_minutes)
        payload, results = summarize_run(
            config=config,
            records=records,
            dry_run=args.dry_run,
            report_mailbox=mailbox_name,
        )

        if args.dry_run:
            print_json(payload)
            return 0

        json_path, markdown_path = write_reports(config, payload)
        processed_at = dt.datetime.now(dt.timezone.utc).isoformat()
        for result in results:
            # 只有在报告成功写出后，才记录 processed 标记，避免“状态已写但报告缺失”。
            record_processed(
                connection,
                mailbox=mailbox_name,
                uidvalidity=uidvalidity,
                result=result,
                report_path=str(markdown_path),
                processed_at=processed_at,
            )
        print(f"Wrote JSON report to {json_path}")
        print(f"Wrote Markdown report to {markdown_path}")
        return 0
    except Exception as exc:  # pragma: no cover - exercised by integration use
        print(f"Run failed: {exc}", file=sys.stderr)
        return 1
    finally:
        if client is not None:
            try:
                client.logout()
            except Exception:
                pass
        if connection is not None:
            connection.close()


if __name__ == "__main__":
    raise SystemExit(main())
