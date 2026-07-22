#!/usr/bin/env python3
"""Fail-closed local socket challenges for the protected Comparator runner."""

from __future__ import annotations

import errno
import socket
import sys
from pathlib import Path


def listen(port_file: Path) -> int:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as listener:
        listener.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        listener.bind(("127.0.0.1", 0))
        listener.listen(1)
        port_file.write_text(f"{listener.getsockname()[1]}\n", encoding="ascii")
        listener.settimeout(60)
        try:
            connection, _ = listener.accept()
        except TimeoutError:
            return 0
        with connection:
            print("sandboxed TCP connection unexpectedly succeeded", file=sys.stderr)
            return 1


def require_tcp_denial(port: int) -> int:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as client:
        try:
            client.connect(("127.0.0.1", port))
        except OSError as error:
            if error.errno in {errno.EACCES, errno.EPERM}:
                print("MATHOS_LANDRUN_TCP_DENIED=passed")
                return 0
            print(f"unexpected TCP challenge error: {error}", file=sys.stderr)
            return 1
        print("landrun allowed a forbidden TCP connection", file=sys.stderr)
        return 1


def require_unix_denial() -> int:
    try:
        unix_socket = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    except OSError as error:
        if error.errno in {errno.EACCES, errno.EPERM, errno.EAFNOSUPPORT}:
            print("MATHOS_SYSTEMD_AF_UNIX_DENIED=passed")
            return 0
        print(f"unexpected AF_UNIX challenge error: {error}", file=sys.stderr)
        return 1
    unix_socket.close()
    print("systemd allowed forbidden AF_UNIX socket creation", file=sys.stderr)
    return 1


def main(arguments: list[str]) -> int:
    if len(arguments) == 2 and arguments[0] == "listen":
        return listen(Path(arguments[1]))
    if len(arguments) == 2 and arguments[0] == "tcp":
        try:
            port = int(arguments[1])
        except ValueError:
            print("TCP challenge port is not an integer", file=sys.stderr)
            return 64
        if not 1 <= port <= 65535:
            print("TCP challenge port is out of range", file=sys.stderr)
            return 64
        return require_tcp_denial(port)
    if arguments == ["unix"]:
        return require_unix_denial()
    print("usage: comparator-network-probe.py listen <port-file> | tcp <port> | unix", file=sys.stderr)
    return 64


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
