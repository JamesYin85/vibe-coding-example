#!/usr/bin/env python3
from __future__ import annotations

import json
import os
from pathlib import Path
import sys
import tempfile
import textwrap
from unittest import mock

import run_digest
from triage_lib import EmailRecord


class FakeImapClient:
    def logout(self) -> None:
        return None


def build_record() -> EmailRecord:
    return EmailRecord(
        uid="42",
        mailbox="INBOX",
        uidvalidity="123",
        subject="请处理：客户报价审批",
        from_name="Alice",
        from_address="alice@company.com",
        to_addresses=["me@company.com"],
        cc_addresses=[],
        received_at="2026-03-12T09:30:00+08:00",
        message_id="<abc123@company.com>",
        text_body="请今天中午前处理客户 A 的报价审批，附件已更新。",
        html_body="",
        raw_bytes=b"From: Alice <alice@company.com>\nSubject: test\n",
    )


def write_temp_config(root: Path) -> Path:
    config_path = root / "config.yaml"
    config_path.write_text(
        textwrap.dedent(
            """
            imap_host: "imap.exmail.qq.com"
            imap_port: 993
            imap_ssl: true
            email_address: "me@company.com"
            password_env_var: "EXMAIL_PASSWORD"
            interval_minutes: 15
            llm_provider: "openai"
            llm_api_key_env_var: "OPENAI_API_KEY"
            llm_base_url: "https://api.openai.com/v1"
            model: "gpt-4.1-mini"
            display_name: "工作邮箱摘要"
            aliases:
              - "me@company.com"
              - "james"
            important_senders:
              - "ceo@company.com"
            important_keywords:
              - "请处理"
            general_keywords:
              - "知悉"
            useless_senders:
              - "newsletter@vendor.com"
            useless_keywords:
              - "unsubscribe"
            output_dir: "{output_dir}"
            archive_dir: "{archive_dir}"
            state_db_path: "{state_db_path}"
            timezone: "Asia/Shanghai"
            max_emails_per_run: 50
            """
        ).strip().format(
            output_dir=str(root / "reports"),
            archive_dir=str(root / "archive"),
            state_db_path=str(root / "state" / "triage.sqlite3"),
        )
        + "\n"
    )
    return config_path


def capture_stdout(callable_obj, *args):
    writes: list[str] = []

    def fake_write(chunk: str) -> int:
        writes.append(chunk)
        return len(chunk)

    with mock.patch("sys.stdout.write", side_effect=fake_write):
        exit_code = callable_obj(*args)
    return exit_code, "".join(writes)


def run_self_check() -> int:
    with tempfile.TemporaryDirectory() as tmpdir:
        root = Path(tmpdir)
        config_path = write_temp_config(root)
        record = build_record()
        base_patches = (
            mock.patch.dict(
                os.environ,
                {"EXMAIL_PASSWORD": "dummy-password", "OPENAI_API_KEY": "dummy-key"},
                clear=False,
            ),
            mock.patch.object(run_digest, "connect_imap", return_value=FakeImapClient()),
            mock.patch.object(run_digest, "select_mailbox", return_value="123"),
            mock.patch.object(run_digest, "list_candidate_records", return_value=[record]),
            mock.patch(
                "triage_lib.call_openai_json",
                return_value={
                    "classification": "important",
                    "importance_score": 5,
                    "action_required": True,
                    "reason_tags": ["model-important"],
                    "summary": "Alice 发来客户报价审批邮件，需要尽快处理。",
                    "excerpt": "请今天中午前处理客户 A 的报价审批。",
                },
            ),
        )

        print("[1/3] Running dry-run pipeline check")
        with base_patches[0], base_patches[1], base_patches[2], base_patches[3], base_patches[4]:
            dry_exit, dry_output = capture_stdout(
                run_digest.main,
                ["--config", str(config_path), "--dry-run"],
            )
        if dry_exit != 0:
            print("Dry-run check failed.")
            return 1
        payload = json.loads(dry_output)
        if payload.get("processed_count") != 1:
            print("Dry-run check failed: expected processed_count == 1.")
            return 1

        print("[2/3] Running live-write pipeline check")
        with base_patches[0], base_patches[1], base_patches[2], base_patches[3], base_patches[4]:
            live_exit = run_digest.main(["--config", str(config_path)])
        if live_exit != 0:
            print("Live-write check failed.")
            return 1

        report_files = list((root / "reports").rglob("*-report.json"))
        markdown_files = list((root / "reports").rglob("*-report.md"))
        archive_files = list((root / "archive").rglob("42.eml"))
        state_db = root / "state" / "triage.sqlite3"
        if not report_files or not markdown_files or not archive_files or not state_db.exists():
            print("Live-write check failed: expected reports, archive, and state DB.")
            return 1

        print("[3/3] Verifying output structure")
        report_payload = json.loads(report_files[0].read_text())
        if report_payload["items"][0]["classification"] != "important":
            print("Output verification failed: expected important classification.")
            return 1
        if "## 重要邮件" not in markdown_files[0].read_text():
            print("Output verification failed: Markdown grouping missing.")
            return 1

        print("Self-check passed.")
        print(f"Generated temporary artifacts under {root}")
        return 0


if __name__ == "__main__":
    raise SystemExit(run_self_check())
