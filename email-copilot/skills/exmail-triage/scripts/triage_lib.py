#!/usr/bin/env python3
from __future__ import annotations

import argparse
import dataclasses
import datetime as dt
import email
from email.message import Message
from email.parser import BytesParser
from email.policy import default as default_policy
from email.utils import getaddresses, parsedate_to_datetime
import html
from html.parser import HTMLParser
import imaplib
import json
import os
from pathlib import Path
import re
import sqlite3
import sys
from typing import Any, Iterable
import urllib.error
import urllib.request
from zoneinfo import ZoneInfo, ZoneInfoNotFoundError

try:
    import yaml
except ImportError:  # pragma: no cover - exercised only when dependency is missing
    yaml = None


DEFAULT_IMPORTANT_KEYWORDS = [
    "action required",
    "asap",
    "important",
    "please handle",
    "please review",
    "urgent",
    "请处理",
    "请尽快",
    "紧急",
    "重要",
]
DEFAULT_GENERAL_KEYWORDS = [
    "fyi",
    "for your information",
    "notice",
    "weekly report",
    "会议纪要",
    "周报",
    "抄送",
    "知悉",
    "通知",
]
DEFAULT_USELESS_KEYWORDS = [
    "digest",
    "newsletter",
    "promotion",
    "subscription",
    "unsubscribe",
    "验证码",
    "促销",
    "订阅",
    "营销",
]
DEFAULT_USELESS_SENDERS = [
    "mailer-daemon",
    "no-reply",
    "noreply",
]
JSON_SCHEMA_HINT = {
    "classification": "important | general | useless",
    "importance_score": "integer 1-5",
    "action_required": "boolean",
    "reason_tags": ["array", "of", "short-tags"],
    "summary": "short summary under 280 characters",
    "excerpt": "short excerpt under 200 characters",
}


class ConfigError(ValueError):
    pass


class ModelError(RuntimeError):
    pass


@dataclasses.dataclass(slots=True)
class TriageConfig:
    imap_host: str
    imap_port: int
    imap_ssl: bool
    email_address: str
    password_env_var: str
    interval_minutes: int
    llm_provider: str
    llm_api_key_env_var: str
    model: str
    display_name: str
    aliases: list[str]
    important_senders: list[str]
    important_keywords: list[str]
    general_keywords: list[str]
    useless_senders: list[str]
    useless_keywords: list[str]
    output_dir: Path
    archive_dir: Path
    state_db_path: Path
    timezone: str
    max_emails_per_run: int
    llm_base_url: str = "https://api.openai.com/v1/chat/completions"


@dataclasses.dataclass(slots=True)
class EmailRecord:
    uid: str
    mailbox: str
    uidvalidity: str
    subject: str
    from_name: str
    from_address: str
    to_addresses: list[str]
    cc_addresses: list[str]
    received_at: str
    message_id: str
    text_body: str
    html_body: str
    raw_bytes: bytes


@dataclasses.dataclass(slots=True)
class TriageResult:
    uid: str
    message_id: str
    subject: str
    from_name: str
    from_address: str
    received_at: str
    classification: str
    importance_score: int
    action_required: bool
    reason_tags: list[str]
    summary: str
    excerpt: str
    archived_path: str


class _HTMLStripper(HTMLParser):
    def __init__(self) -> None:
        super().__init__()
        self._parts: list[str] = []

    def handle_data(self, data: str) -> None:
        self._parts.append(data)

    def get_text(self) -> str:
        return "".join(self._parts)


def parse_bool(value: Any) -> bool:
    if isinstance(value, bool):
        return value
    if isinstance(value, str):
        lowered = value.strip().lower()
        if lowered in {"1", "true", "yes", "on"}:
            return True
        if lowered in {"0", "false", "no", "off"}:
            return False
    raise ConfigError(f"Expected a boolean value, got: {value!r}")


def normalize_list(values: Any, *, field_name: str) -> list[str]:
    if values is None:
        return []
    if not isinstance(values, list):
        raise ConfigError(f"Expected '{field_name}' to be a list of strings.")
    normalized: list[str] = []
    for value in values:
        if not isinstance(value, str):
            raise ConfigError(f"Expected every entry in '{field_name}' to be a string.")
        stripped = value.strip()
        if stripped:
            normalized.append(stripped)
    return normalized


def load_config_data(config_path: str | os.PathLike[str]) -> dict[str, Any]:
    if yaml is None:
        raise ConfigError("PyYAML is required to load YAML config files.")
    path = Path(config_path)
    if not path.exists():
        raise ConfigError(f"Config file not found: {path}")
    payload = yaml.safe_load(path.read_text())
    if not isinstance(payload, dict):
        raise ConfigError("Config root must be a YAML mapping.")
    return payload


def _compat_value(data: dict[str, Any], key: str, fallback_key: str) -> Any:
    if key in data:
        return data[key]
    return data.get(fallback_key)


def _compat_key_present(data: dict[str, Any], key: str, fallback_key: str) -> bool:
    return key in data or fallback_key in data


def normalize_chat_completions_url(value: str) -> str:
    # 兼容两种写法：既支持直接传 chat/completions，也支持只传兼容的 API 根路径。
    url = value.strip()
    if not url:
        raise ConfigError("Expected a non-empty LLM base URL.")
    normalized = url.rstrip("/")
    if normalized.endswith("/chat/completions"):
        return normalized
    if normalized.endswith("/v1"):
        return f"{normalized}/chat/completions"
    if normalized.endswith("/v4"):
        return f"{normalized}/chat/completions"
    return normalized


def validate_config_data(data: dict[str, Any], *, check_env: bool = True) -> list[str]:
    required_fields = {
        "imap_host": str,
        "imap_port": int,
        "imap_ssl": bool,
        "email_address": str,
        "password_env_var": str,
        "interval_minutes": int,
        "model": str,
        "display_name": str,
        "aliases": list,
        "important_senders": list,
        "important_keywords": list,
        "general_keywords": list,
        "useless_senders": list,
        "useless_keywords": list,
        "output_dir": str,
        "archive_dir": str,
        "state_db_path": str,
        "timezone": str,
        "max_emails_per_run": int,
    }
    errors: list[str] = []
    for key, expected_type in required_fields.items():
        if key not in data:
            errors.append(f"Missing required config key: {key}")
            continue
        value = data[key]
        if expected_type is bool:
            try:
                parse_bool(value)
            except ConfigError as exc:
                errors.append(str(exc))
        elif expected_type is list:
            if not isinstance(value, list):
                errors.append(f"Expected '{key}' to be a list.")
        elif expected_type is int:
            if not isinstance(value, int):
                errors.append(f"Expected '{key}' to be an integer.")
        elif expected_type is str:
            if not isinstance(value, str) or not value.strip():
                errors.append(f"Expected '{key}' to be a non-empty string.")

    llm_provider = _compat_value(data, "llm_provider", "provider")
    if llm_provider is not None and (not isinstance(llm_provider, str) or not llm_provider.strip()):
        errors.append("Expected 'llm_provider' to be a non-empty string when provided.")

    if not _compat_key_present(data, "llm_api_key_env_var", "openai_api_key_env_var"):
        errors.append("Missing required config key: llm_api_key_env_var")
    else:
        llm_api_key_env_var = _compat_value(data, "llm_api_key_env_var", "openai_api_key_env_var")
        if not isinstance(llm_api_key_env_var, str) or not llm_api_key_env_var.strip():
            errors.append("Expected 'llm_api_key_env_var' to be a non-empty string.")

    llm_base_url = _compat_value(data, "llm_base_url", "openai_base_url")
    if llm_base_url is not None:
        if not isinstance(llm_base_url, str) or not llm_base_url.strip():
            errors.append("Expected 'llm_base_url' to be a non-empty string.")
        else:
            try:
                normalize_chat_completions_url(llm_base_url)
            except ConfigError as exc:
                errors.append(str(exc))

    timezone_value = data.get("timezone")
    if isinstance(timezone_value, str) and timezone_value.strip():
        try:
            ZoneInfo(timezone_value)
        except (ZoneInfoNotFoundError, ValueError):
            errors.append(f"Unknown timezone: {timezone_value!r}")

    for key in ("output_dir", "archive_dir"):
        value = data.get(key)
        if isinstance(value, str) and not value.strip():
            errors.append(f"Expected '{key}' to be a non-empty path.")

    for key in ("interval_minutes", "max_emails_per_run"):
        value = data.get(key)
        if isinstance(value, int) and value <= 0:
            errors.append(f"Expected '{key}' to be greater than zero.")

    if check_env:
        for env_key in ("password_env_var",):
            env_name = data.get(env_key)
            if isinstance(env_name, str) and env_name.strip() and not os.getenv(env_name):
                errors.append(f"Environment variable is not set: {env_name}")
        llm_env_name = _compat_value(data, "llm_api_key_env_var", "openai_api_key_env_var")
        if isinstance(llm_env_name, str) and llm_env_name.strip() and not os.getenv(llm_env_name):
            errors.append(f"Environment variable is not set: {llm_env_name}")

    return errors


def load_config(config_path: str | os.PathLike[str], *, check_env: bool = True) -> TriageConfig:
    """加载并校验 YAML 配置，返回后续流程可直接使用的强类型配置对象。"""
    data = load_config_data(config_path)
    errors = validate_config_data(data, check_env=check_env)
    if errors:
        raise ConfigError("\n".join(errors))

    # 在入口统一做类型归一化，后续逻辑就不需要重复处理原始 YAML 值。
    return TriageConfig(
        imap_host=data["imap_host"].strip(),
        imap_port=int(data["imap_port"]),
        imap_ssl=parse_bool(data["imap_ssl"]),
        email_address=data["email_address"].strip(),
        password_env_var=data["password_env_var"].strip(),
        interval_minutes=int(data["interval_minutes"]),
        llm_provider=str(_compat_value(data, "llm_provider", "provider") or "openai").strip().lower(),
        llm_api_key_env_var=str(_compat_value(data, "llm_api_key_env_var", "openai_api_key_env_var")).strip(),
        model=data["model"].strip(),
        display_name=data["display_name"].strip(),
        aliases=normalize_list(data.get("aliases"), field_name="aliases"),
        important_senders=normalize_list(data.get("important_senders"), field_name="important_senders"),
        important_keywords=normalize_list(data.get("important_keywords"), field_name="important_keywords"),
        general_keywords=normalize_list(data.get("general_keywords"), field_name="general_keywords"),
        useless_senders=normalize_list(data.get("useless_senders"), field_name="useless_senders"),
        useless_keywords=normalize_list(data.get("useless_keywords"), field_name="useless_keywords"),
        output_dir=Path(data["output_dir"]).expanduser(),
        archive_dir=Path(data["archive_dir"]).expanduser(),
        state_db_path=Path(data["state_db_path"]).expanduser(),
        timezone=data["timezone"].strip(),
        max_emails_per_run=int(data["max_emails_per_run"]),
        llm_base_url=normalize_chat_completions_url(
            str(_compat_value(data, "llm_base_url", "openai_base_url") or "https://api.openai.com/v1/chat/completions")
        ),
    )


def ensure_output_paths(config: TriageConfig) -> None:
    config.output_dir.mkdir(parents=True, exist_ok=True)
    config.archive_dir.mkdir(parents=True, exist_ok=True)
    config.state_db_path.parent.mkdir(parents=True, exist_ok=True)


def initialize_state_db(db_path: Path) -> sqlite3.Connection:
    """初始化状态库，用于跨多次运行记录已处理邮件。"""
    connection = sqlite3.connect(db_path)
    connection.execute(
        """
        CREATE TABLE IF NOT EXISTS processed_emails (
            mailbox TEXT NOT NULL,
            uidvalidity TEXT NOT NULL,
            uid TEXT NOT NULL,
            message_id TEXT NOT NULL,
            received_at TEXT NOT NULL,
            classification TEXT NOT NULL,
            report_path TEXT NOT NULL,
            archived_path TEXT NOT NULL,
            processed_at TEXT NOT NULL,
            PRIMARY KEY (mailbox, uidvalidity, uid)
        )
        """
    )
    # 先提交建表，确保后续运行都能依赖同一套去重约束。
    connection.commit()
    return connection


def has_processed(connection: sqlite3.Connection, mailbox: str, uidvalidity: str, uid: str) -> bool:
    cursor = connection.execute(
        """
        SELECT 1
        FROM processed_emails
        WHERE mailbox = ? AND uidvalidity = ? AND uid = ?
        LIMIT 1
        """,
        (mailbox, uidvalidity, uid),
    )
    return cursor.fetchone() is not None


def record_processed(
    connection: sqlite3.Connection,
    *,
    mailbox: str,
    uidvalidity: str,
    result: TriageResult,
    report_path: str,
    processed_at: str,
) -> None:
    connection.execute(
        """
        INSERT OR REPLACE INTO processed_emails (
            mailbox,
            uidvalidity,
            uid,
            message_id,
            received_at,
            classification,
            report_path,
            archived_path,
            processed_at
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
        """,
        (
            mailbox,
            uidvalidity,
            result.uid,
            result.message_id,
            result.received_at,
            result.classification,
            report_path,
            result.archived_path,
            processed_at,
        ),
    )
    connection.commit()


def connect_imap(config: TriageConfig) -> imaplib.IMAP4:
    if config.imap_ssl:
        mailbox = imaplib.IMAP4_SSL(config.imap_host, config.imap_port)
    else:
        mailbox = imaplib.IMAP4(config.imap_host, config.imap_port)
    password = os.getenv(config.password_env_var)
    if not password:
        raise ConfigError(f"Environment variable is not set: {config.password_env_var}")
    mailbox.login(config.email_address, password)
    return mailbox


def select_mailbox(client: imaplib.IMAP4, mailbox_name: str = "INBOX") -> str:
    """选择目标邮箱，并返回用于幂等去重的 UIDVALIDITY。"""
    status, _ = client.select(mailbox_name, readonly=True)
    if status != "OK":
        raise RuntimeError(f"Unable to select mailbox: {mailbox_name}")
    response = client.response("UIDVALIDITY")
    uidvalidity = ""
    # UIDVALIDITY 用于区分邮箱状态世代，避免 UID 在邮箱重建后发生语义冲突。
    if response and response[1]:
        raw = response[1][0]
        uidvalidity = raw.decode() if isinstance(raw, bytes) else str(raw)
    return uidvalidity or "unknown"


def search_unseen_uids(client: imaplib.IMAP4) -> list[str]:
    status, data = client.uid("search", None, "UNSEEN")
    if status != "OK":
        raise RuntimeError("Unable to search mailbox for unseen messages.")
    raw = data[0].decode() if data and data[0] else ""
    return [token for token in raw.split() if token]


def _extract_fetch_bytes(fetch_data: list[Any]) -> bytes:
    chunks: list[bytes] = []
    for item in fetch_data:
        if isinstance(item, tuple) and len(item) >= 2 and isinstance(item[1], bytes):
            chunks.append(item[1])
    return b"".join(chunks)


def fetch_email_record(
    client: imaplib.IMAP4,
    uid: str,
    *,
    mailbox: str,
    uidvalidity: str,
    timezone_name: str,
) -> EmailRecord:
    """按 UID 拉取一封原始邮件，并抽取后续分级所需的结构化字段。"""
    status, data = client.uid("fetch", uid, "(RFC822)")
    if status != "OK":
        raise RuntimeError(f"Unable to fetch message UID {uid}.")
    raw_bytes = _extract_fetch_bytes(data)
    if not raw_bytes:
        raise RuntimeError(f"Fetched empty payload for UID {uid}.")

    # 一次性解析完整 RFC822 内容，同时保留结构化字段和原始字节，后面归档直接复用。
    message = BytesParser(policy=default_policy).parsebytes(raw_bytes)
    from_pairs = getaddresses(message.get_all("from", []))
    to_pairs = getaddresses(message.get_all("to", []))
    cc_pairs = getaddresses(message.get_all("cc", []))
    from_name, from_address = from_pairs[0] if from_pairs else ("", "")

    received_dt = parse_received_datetime(message.get("date"), timezone_name)
    text_body, html_body = extract_message_bodies(message)
    return EmailRecord(
        uid=uid,
        mailbox=mailbox,
        uidvalidity=uidvalidity,
        subject=(message.get("subject") or "(no subject)").strip(),
        from_name=(from_name or "").strip(),
        from_address=(from_address or "").strip().lower(),
        to_addresses=[address.lower() for _, address in to_pairs if address],
        cc_addresses=[address.lower() for _, address in cc_pairs if address],
        received_at=received_dt.isoformat(),
        message_id=(message.get("message-id") or f"<uid-{uid}>").strip(),
        text_body=text_body,
        html_body=html_body,
        raw_bytes=raw_bytes,
    )


def parse_received_datetime(value: str | None, timezone_name: str) -> dt.datetime:
    zone = ZoneInfo(timezone_name)
    if not value:
        return dt.datetime.now(tz=zone)
    parsed = parsedate_to_datetime(value)
    if parsed.tzinfo is None:
        parsed = parsed.replace(tzinfo=dt.timezone.utc)
    return parsed.astimezone(zone)


def extract_message_bodies(message: Message) -> tuple[str, str]:
    """提取邮件正文，优先纯文本，没有纯文本时再从 HTML 回退。"""
    text_parts: list[str] = []
    html_parts: list[str] = []

    if message.is_multipart():
        for part in message.walk():
            disposition = (part.get_content_disposition() or "").lower()
            # 分级只关注头部和正文，附件内容明确不参与本轮处理。
            if disposition == "attachment":
                continue
            content_type = part.get_content_type()
            try:
                payload = part.get_content()
            except LookupError:
                # 遇到标准库不认识的历史编码时，退回到宽松的字节解码，避免整封邮件失败。
                payload = part.get_payload(decode=True) or b""
                charset = part.get_content_charset() or "utf-8"
                if isinstance(payload, bytes):
                    payload = payload.decode(charset, errors="replace")
            if not isinstance(payload, str):
                continue
            if content_type == "text/plain":
                text_parts.append(payload)
            elif content_type == "text/html":
                html_parts.append(payload)
    else:
        payload = message.get_content()
        if isinstance(payload, str):
            if message.get_content_type() == "text/html":
                html_parts.append(payload)
            else:
                text_parts.append(payload)

    text_body = "\n\n".join(part.strip() for part in text_parts if part.strip())
    html_body = "\n\n".join(part.strip() for part in html_parts if part.strip())
    if not text_body and html_body:
        # 优先使用 text/plain，但对纯 HTML 邮件仍要给下游提供可分级的文本内容。
        text_body = html_to_text(html_body)
    return collapse_whitespace(text_body), collapse_whitespace(html_body)


def html_to_text(payload: str) -> str:
    stripper = _HTMLStripper()
    stripper.feed(payload)
    return collapse_whitespace(html.unescape(stripper.get_text()))


def collapse_whitespace(value: str) -> str:
    return re.sub(r"\s+", " ", value or "").strip()


def classification_keywords(config: TriageConfig) -> dict[str, list[str]]:
    return {
        "important": sorted({*(keyword.lower() for keyword in DEFAULT_IMPORTANT_KEYWORDS), *(keyword.lower() for keyword in config.important_keywords)}),
        "general": sorted({*(keyword.lower() for keyword in DEFAULT_GENERAL_KEYWORDS), *(keyword.lower() for keyword in config.general_keywords)}),
        "useless": sorted({*(keyword.lower() for keyword in DEFAULT_USELESS_KEYWORDS), *(keyword.lower() for keyword in config.useless_keywords)}),
    }


def sender_rules(config: TriageConfig) -> dict[str, list[str]]:
    return {
        "important": sorted({sender.lower() for sender in config.important_senders}),
        "useless": sorted({*(sender.lower() for sender in DEFAULT_USELESS_SENDERS), *(sender.lower() for sender in config.useless_senders)}),
    }


def _contains_any(text: str, phrases: Iterable[str]) -> list[str]:
    lowered = text.lower()
    hits: list[str] = []
    for phrase in phrases:
        token = phrase.lower().strip()
        if token and token in lowered:
            hits.append(token)
    return hits


def _mention_aliases(record: EmailRecord, config: TriageConfig) -> list[str]:
    aliases = {config.email_address.lower(), *(alias.lower() for alias in config.aliases)}
    local_parts = {alias.split("@", 1)[0] for alias in aliases if alias}
    content = f"{record.subject} {record.text_body}".lower()
    hits: list[str] = []
    for token in sorted(local_parts):
        if token and f"@{token}" in content:
            hits.append(f"mention:{token}")
    return hits


def rule_based_triage(record: EmailRecord, config: TriageConfig) -> tuple[str, int, bool, list[str]]:
    """基于收件关系、发件人和关键词先做一轮确定性分级。"""
    aliases = {config.email_address.lower(), *(alias.lower() for alias in config.aliases)}
    keywords = classification_keywords(config)
    senders = sender_rules(config)
    content = f"{record.subject}\n{record.text_body}"
    reason_tags: list[str] = []
    to_me = any(address in aliases for address in record.to_addresses)
    cc_me = any(address in aliases for address in record.cc_addresses)

    if to_me:
        reason_tags.append("direct-recipient")
    if cc_me:
        reason_tags.append("cc-recipient")

    reason_tags.extend(_mention_aliases(record, config))
    important_hits = _contains_any(content, keywords["important"])
    general_hits = _contains_any(content, keywords["general"])
    useless_hits = _contains_any(content, keywords["useless"])

    if record.from_address in senders["important"]:
        reason_tags.append("important-sender")
    if any(sender in record.from_address for sender in senders["useless"]):
        reason_tags.append("bulk-sender")

    if important_hits:
        reason_tags.extend(f"important-keyword:{hit}" for hit in important_hits[:3])
    if general_hits:
        reason_tags.extend(f"general-keyword:{hit}" for hit in general_hits[:3])
    if useless_hits:
        reason_tags.extend(f"useless-keyword:{hit}" for hit in useless_hits[:3])

    # 这里的优先级是刻意设计的：直达本人和明显重要信号，应当压过群发/噪音特征。
    if to_me or "important-sender" in reason_tags or important_hits or _mention_aliases(record, config):
        return "important", 5 if to_me or important_hits else 4, True, dedupe_list(reason_tags)
    if "bulk-sender" in reason_tags or useless_hits:
        return "useless", 1, False, dedupe_list(reason_tags)
    if cc_me or general_hits:
        return "general", 3, False, dedupe_list(reason_tags)
    return "general", 2, False, dedupe_list(reason_tags or ["default-general"])


def dedupe_list(values: Iterable[str]) -> list[str]:
    seen: set[str] = set()
    deduped: list[str] = []
    for value in values:
        if value and value not in seen:
            deduped.append(value)
            seen.add(value)
    return deduped


def fallback_summary(record: EmailRecord, classification: str, reason_tags: list[str]) -> tuple[str, str]:
    content = record.text_body or html_to_text(record.html_body)
    excerpt = content[:200].strip()
    if len(content) > 200:
        excerpt = f"{excerpt.rstrip()}..."
    sender = record.from_name or record.from_address or "unknown sender"
    reason_text = ", ".join(reason_tags[:3]) if reason_tags else "default heuristics"
    summary = f"{sender}: {record.subject}. Classified as {classification} based on {reason_text}."
    return truncate(summary, 280), truncate(excerpt or record.subject, 200)


def truncate(value: str, limit: int) -> str:
    value = collapse_whitespace(value)
    if len(value) <= limit:
        return value
    return value[: limit - 3].rstrip() + "..."


def call_openai_json(
    *,
    record: EmailRecord,
    config: TriageConfig,
    baseline_classification: str,
    baseline_score: int,
    baseline_action_required: bool,
    reason_tags: list[str],
) -> dict[str, Any]:
    """调用兼容 OpenAI 的接口，让模型在规则基线上补充摘要和细化分类。"""
    api_key = os.getenv(config.llm_api_key_env_var)
    if not api_key:
        raise ModelError(f"Environment variable is not set: {config.llm_api_key_env_var}")

    # Prompt 里带上规则分级结果，约束模型做“细化”而不是完全推翻本地启发式判断。
    prompt = {
        "task": "Classify and summarize an email for a personal digest.",
        "baseline": {
            "classification": baseline_classification,
            "importance_score": baseline_score,
            "action_required": baseline_action_required,
            "reason_tags": reason_tags,
        },
        "rules": {
            "do_not_downgrade_important_to_useless": True,
            "keep_useless_as_useless_when_rule_based": baseline_classification == "useless",
            "classification_values": ["important", "general", "useless"],
        },
        "output_schema": JSON_SCHEMA_HINT,
        "email": {
            "subject": record.subject,
            "from_name": record.from_name,
            "from_address": record.from_address,
            "to_addresses": record.to_addresses,
            "cc_addresses": record.cc_addresses,
            "received_at": record.received_at,
            "body_excerpt": truncate(record.text_body or html_to_text(record.html_body), 4000),
        },
    }
    payload = {
        "model": config.model,
        "response_format": {"type": "json_object"},
        "temperature": 0.2,
        "messages": [
            {
                "role": "system",
                "content": (
                    "You classify work emails into important, general, or useless and return JSON only."
                ),
            },
            {
                "role": "user",
                "content": json.dumps(prompt, ensure_ascii=False),
            },
        ],
    }
    request = urllib.request.Request(
        config.llm_base_url,
        data=json.dumps(payload).encode("utf-8"),
        headers={
            "Authorization": f"Bearer {api_key}",
            "Content-Type": "application/json",
        },
        method="POST",
    )
    try:
        # 给模型请求加超时，避免单封邮件响应过慢把整轮摘要长期卡住。
        with urllib.request.urlopen(request, timeout=30) as response:
            response_data = json.loads(response.read().decode("utf-8"))
    except (urllib.error.URLError, TimeoutError, json.JSONDecodeError) as exc:
        raise ModelError(f"OpenAI request failed: {exc}") from exc

    try:
        content = response_data["choices"][0]["message"]["content"]
    except (KeyError, IndexError, TypeError) as exc:
        raise ModelError("OpenAI response did not include message content.") from exc

    try:
        parsed = json.loads(content)
    except json.JSONDecodeError as exc:
        raise ModelError(f"OpenAI response was not valid JSON: {exc}") from exc

    if not isinstance(parsed, dict):
        raise ModelError("OpenAI JSON payload must be an object.")
    return parsed


def apply_model_guardrails(
    *,
    baseline_classification: str,
    baseline_score: int,
    baseline_action_required: bool,
    baseline_tags: list[str],
    model_payload: dict[str, Any],
) -> tuple[str, int, bool, list[str], str, str]:
    """对模型返回结果做边界收敛，确保不违反业务规则。"""
    classification = str(model_payload.get("classification", baseline_classification)).strip().lower()
    if classification not in {"important", "general", "useless"}:
        classification = baseline_classification

    # 模型可以微调结论，但不能突破“important 不降 useless、useless 保持 useless”这些硬规则。
    if baseline_classification == "important" and classification == "useless":
        classification = "important"
    if baseline_classification == "useless":
        classification = "useless"

    importance_score = model_payload.get("importance_score", baseline_score)
    if not isinstance(importance_score, int):
        importance_score = baseline_score
    importance_score = max(1, min(5, importance_score))
    if baseline_classification == "important":
        importance_score = max(importance_score, baseline_score)
    if baseline_classification == "useless":
        importance_score = min(importance_score, 2)

    action_required = model_payload.get("action_required", baseline_action_required)
    if not isinstance(action_required, bool):
        action_required = baseline_action_required
    if baseline_classification == "important":
        action_required = action_required or baseline_action_required
    if classification == "useless":
        action_required = False

    reason_tags = model_payload.get("reason_tags", [])
    if not isinstance(reason_tags, list):
        reason_tags = []
    normalized_tags = [str(tag).strip() for tag in reason_tags if str(tag).strip()]
    merged_tags = dedupe_list([*baseline_tags, *normalized_tags])

    summary = truncate(str(model_payload.get("summary", "")).strip(), 280)
    excerpt = truncate(str(model_payload.get("excerpt", "")).strip(), 200)
    return (
        classification,
        importance_score,
        action_required,
        merged_tags,
        summary,
        excerpt,
    )


def triage_record(record: EmailRecord, config: TriageConfig) -> TriageResult:
    """完成单封邮件的规则分级、模型增强和失败兜底。"""
    baseline_classification, baseline_score, baseline_action_required, reason_tags = rule_based_triage(record, config)
    if baseline_classification == "useless":
        # 明显低价值邮件直接跳过模型，降低整体耗时和 token 成本。
        summary, excerpt = fallback_summary(record, baseline_classification, reason_tags)
        return TriageResult(
            uid=record.uid,
            message_id=record.message_id,
            subject=record.subject,
            from_name=record.from_name,
            from_address=record.from_address,
            received_at=record.received_at,
            classification=baseline_classification,
            importance_score=baseline_score,
            action_required=False,
            reason_tags=reason_tags,
            summary=summary,
            excerpt=excerpt,
            archived_path="",
        )

    try:
        model_payload = call_openai_json(
            record=record,
            config=config,
            baseline_classification=baseline_classification,
            baseline_score=baseline_score,
            baseline_action_required=baseline_action_required,
            reason_tags=reason_tags,
        )
        classification, importance_score, action_required, merged_tags, summary, excerpt = apply_model_guardrails(
            baseline_classification=baseline_classification,
            baseline_score=baseline_score,
            baseline_action_required=baseline_action_required,
            baseline_tags=reason_tags,
            model_payload=model_payload,
        )
        if not summary or not excerpt:
            fallback = fallback_summary(record, classification, merged_tags)
            summary = summary or fallback[0]
            excerpt = excerpt or fallback[1]
    except ModelError:
        # 模型失败不能拖垮整轮任务，退回确定性摘要继续产出结果。
        merged_tags = dedupe_list([*reason_tags, "model-fallback"])
        summary, excerpt = fallback_summary(record, baseline_classification, merged_tags)
        classification = baseline_classification
        importance_score = baseline_score
        action_required = baseline_action_required

    return TriageResult(
        uid=record.uid,
        message_id=record.message_id,
        subject=record.subject,
        from_name=record.from_name,
        from_address=record.from_address,
        received_at=record.received_at,
        classification=classification,
        importance_score=importance_score,
        action_required=action_required,
        reason_tags=merged_tags,
        summary=summary,
        excerpt=excerpt,
        archived_path="",
    )


def archive_email(record: EmailRecord, config: TriageConfig) -> Path:
    received_dt = dt.datetime.fromisoformat(record.received_at)
    archive_path = (
        config.archive_dir
        / received_dt.strftime("%Y")
        / received_dt.strftime("%m")
        / received_dt.strftime("%d")
        / f"{record.uid}.eml"
    )
    archive_path.parent.mkdir(parents=True, exist_ok=True)
    archive_path.write_bytes(record.raw_bytes)
    return archive_path


def build_report_payload(
    *,
    generated_at: str,
    mailbox: str,
    items: list[TriageResult],
) -> dict[str, Any]:
    counts = {"important": 0, "general": 0, "useless": 0}
    for item in items:
        counts[item.classification] = counts.get(item.classification, 0) + 1
    return {
        "generated_at": generated_at,
        "mailbox": mailbox,
        "processed_count": len(items),
        "counts": counts,
        "items": [dataclasses.asdict(item) for item in items],
    }


def render_markdown_report(payload: dict[str, Any], *, display_name: str) -> str:
    lines = [
        f"# {display_name} Mail Digest",
        "",
        f"- Generated at: {payload['generated_at']}",
        f"- Mailbox: {payload['mailbox']}",
        f"- Processed emails: {payload['processed_count']}",
        "",
    ]
    groups = {
        "important": "重要邮件",
        "general": "一般邮件",
        "useless": "无用邮件",
    }
    items = payload["items"]
    if not items:
        lines.extend(["No new emails matched this run.", ""])
        return "\n".join(lines)

    for key in ("important", "general", "useless"):
        lines.extend([f"## {groups[key]}", ""])
        group_items = [item for item in items if item["classification"] == key]
        if not group_items:
            lines.extend(["- None", ""])
            continue
        for item in group_items:
            sender = item["from_name"] or item["from_address"]
            lines.extend(
                [
                    f"### {item['subject']}",
                    f"- 发件人: {sender} <{item['from_address']}>",
                    f"- 接收时间: {item['received_at']}",
                    f"- 重要程度: {item['importance_score']}",
                    f"- 需要处理: {'yes' if item['action_required'] else 'no'}",
                    f"- 原文路径: {item['archived_path'] or '(dry-run)'}",
                    f"- 标签: {', '.join(item['reason_tags']) if item['reason_tags'] else 'none'}",
                    f"- 摘要: {item['summary']}",
                    f"- 摘录: {item['excerpt']}",
                    "",
                ]
            )
    return "\n".join(lines)


def write_reports(config: TriageConfig, payload: dict[str, Any]) -> tuple[Path, Path]:
    generated_dt = dt.datetime.fromisoformat(payload["generated_at"])
    day_dir = config.output_dir / generated_dt.strftime("%Y-%m-%d")
    day_dir.mkdir(parents=True, exist_ok=True)
    basename = generated_dt.strftime("%H%M%S-report")
    json_path = day_dir / f"{basename}.json"
    markdown_path = day_dir / f"{basename}.md"
    json_path.write_text(json.dumps(payload, ensure_ascii=False, indent=2) + "\n")
    markdown_path.write_text(render_markdown_report(payload, display_name=config.display_name) + "\n")
    return json_path, markdown_path


def filter_recent_records(records: list[EmailRecord], *, since_minutes: int | None) -> list[EmailRecord]:
    if since_minutes is None:
        return records
    cutoff = dt.datetime.now(tz=dt.timezone.utc) - dt.timedelta(minutes=since_minutes)
    filtered: list[EmailRecord] = []
    for record in records:
        received_dt = dt.datetime.fromisoformat(record.received_at)
        if received_dt.astimezone(dt.timezone.utc) >= cutoff:
            filtered.append(record)
    return filtered


def list_candidate_records(
    client: imaplib.IMAP4,
    *,
    mailbox: str,
    uidvalidity: str,
    timezone_name: str,
    limit: int,
) -> list[EmailRecord]:
    uids = search_unseen_uids(client)
    if limit:
        uids = uids[:limit]
    return [
        fetch_email_record(
            client,
            uid,
            mailbox=mailbox,
            uidvalidity=uidvalidity,
            timezone_name=timezone_name,
        )
        for uid in uids
    ]


def summarize_run(
    *,
    config: TriageConfig,
    records: list[EmailRecord],
    dry_run: bool,
    report_mailbox: str,
) -> tuple[dict[str, Any], list[TriageResult]]:
    """汇总本轮候选邮件，在正式模式下同时完成原文归档。"""
    results: list[TriageResult] = []
    for record in records:
        result = triage_record(record, config)
        if not dry_run:
            # 先完成分级，再归档原文，这样报告里能回填准确的归档路径。
            archive_path = archive_email(record, config)
            result.archived_path = str(archive_path)
        results.append(result)
    generated_at = dt.datetime.now(tz=ZoneInfo(config.timezone)).isoformat()
    payload = build_report_payload(generated_at=generated_at, mailbox=report_mailbox, items=results)
    return payload, results


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Poll Exmail IMAP inbox and build a digest report.")
    parser.add_argument("--config", required=True, help="Path to YAML config file.")
    parser.add_argument("--dry-run", action="store_true", help="Process email candidates without writing archives or state.")
    parser.add_argument("--limit", type=int, help="Maximum emails to process in this run.")
    parser.add_argument("--since-minutes", type=int, help="Only include messages received within the last N minutes.")
    return parser.parse_args(argv)


def print_json(data: Any) -> None:
    json.dump(data, sys.stdout, ensure_ascii=False, indent=2)
    sys.stdout.write("\n")
