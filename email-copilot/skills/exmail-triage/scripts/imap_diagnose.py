#!/usr/bin/env python3
from __future__ import annotations

import argparse
import imaplib
import os
import socket
import ssl
import sys
from typing import Any

from triage_lib import ConfigError, load_config, select_mailbox


def print_step(step: int, total: int, message: str) -> None:
    print(f"[{step}/{total}] {message}")


def decode_bytes(value: Any) -> str:
    if isinstance(value, bytes):
        return value.decode(errors="replace")
    return str(value)


def diagnose_dns(host: str, port: int) -> None:
    infos = socket.getaddrinfo(host, port, type=socket.SOCK_STREAM)
    addresses: list[str] = []
    for info in infos:
        sockaddr = info[4]
        if sockaddr:
            addresses.append(str(sockaddr[0]))
    unique = sorted(set(addresses))
    print(f"Resolved addresses: {', '.join(unique)}")


def diagnose_tcp(host: str, port: int, timeout: float) -> None:
    with socket.create_connection((host, port), timeout=timeout):
        print(f"TCP connection to {host}:{port} succeeded.")


def diagnose_ssl(host: str, port: int, timeout: float) -> None:
    context = ssl.create_default_context()
    with socket.create_connection((host, port), timeout=timeout) as sock:
        with context.wrap_socket(sock, server_hostname=host) as wrapped:
            print(f"SSL handshake succeeded. Cipher: {wrapped.cipher()[0]}")


def diagnose_imap_greeting(config, timeout: float) -> imaplib.IMAP4:
    if config.imap_ssl:
        client = imaplib.IMAP4_SSL(config.imap_host, config.imap_port, timeout=timeout)
    else:
        client = imaplib.IMAP4(config.imap_host, config.imap_port, timeout=timeout)
    print(f"IMAP greeting received. Welcome: {decode_bytes(client.welcome)}")
    return client


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description="Diagnose Exmail IMAP connectivity step by step: DNS, TCP, SSL, IMAP greeting, login, and INBOX select."
    )
    parser.add_argument("--config", required=True, help="Path to YAML config file.")
    parser.add_argument("--timeout", type=float, default=10.0, help="Socket timeout in seconds.")
    args = parser.parse_args(argv)

    try:
        config = load_config(args.config)
    except ConfigError as exc:
        print(f"Config error:\n{exc}", file=sys.stderr)
        return 1

    password = os.getenv(config.password_env_var)
    if not password:
        print(f"Environment variable is not set: {config.password_env_var}", file=sys.stderr)
        return 1

    client = None
    try:
        print_step(1, 6, f"Resolving {config.imap_host}")
        diagnose_dns(config.imap_host, config.imap_port)

        print_step(2, 6, f"Opening TCP connection to {config.imap_host}:{config.imap_port}")
        diagnose_tcp(config.imap_host, config.imap_port, args.timeout)

        if config.imap_ssl:
            print_step(3, 6, "Performing SSL handshake")
            diagnose_ssl(config.imap_host, config.imap_port, args.timeout)
        else:
            print_step(3, 6, "SSL handshake skipped because imap_ssl=false")

        print_step(4, 6, "Opening IMAP session and reading server greeting")
        client = diagnose_imap_greeting(config, args.timeout)

        print_step(5, 6, "Authenticating with IMAP LOGIN")
        login_result = client.login(config.email_address, password)
        print(f"IMAP login response: {decode_bytes(login_result[0])} {decode_bytes(login_result[1])}")

        print_step(6, 6, "Selecting INBOX")
        uidvalidity = select_mailbox(client, "INBOX")
        print(f"INBOX selected successfully. UIDVALIDITY: {uidvalidity}")
        print("IMAP diagnose passed.")
        return 0
    except Exception as exc:
        print(f"IMAP diagnose failed: {exc}", file=sys.stderr)
        return 1
    finally:
        if client is not None:
            try:
                client.logout()
            except Exception:
                pass


if __name__ == "__main__":
    raise SystemExit(main())
