"""Read-only Codex thread metadata over its measured local WebSocket endpoint.

This deliberately small client handles unfragmented native JSON frames and ping.
Unknown framing fails closed. It never resumes, prompts, approves or cancels a thread.
"""
import base64
import hashlib
import json
import os
import socket
import time


def thread_metadata(path, thread_id, timeout=5, cancelled=lambda: False):
    deadline = time.monotonic() + timeout
    received = 0
    connection = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)

    def check():
        if cancelled() or time.monotonic() >= deadline:
            raise TimeoutError("native_metadata_deadline_or_cancel")

    def read(count):
        nonlocal received
        data = bytearray()
        while len(data) < count:
            check()
            try:
                chunk = connection.recv(min(count - len(data), 65536))
            except socket.timeout:
                continue
            if not chunk:
                raise ValueError("native_metadata_eof")
            received += len(chunk)
            if received > 1024 * 1024:
                raise ValueError("native_metadata_limit")
            data.extend(chunk)
        return bytes(data)

    def send(payload, opcode=1):
        check()
        if len(payload) > 65535:
            raise ValueError("native_metadata_input_limit")
        mask = os.urandom(4)
        length = len(payload)
        prefix = bytes((128 | opcode, 128 | length)) if length < 126 else bytes((128 | opcode, 254)) + length.to_bytes(2, "big")
        connection.sendall(prefix + mask + bytes(value ^ mask[index % 4] for index, value in enumerate(payload)))

    def message(value):
        send(json.dumps(value, separators=(",", ":")).encode())

    def receive():
        while True:
            prefix = read(2)
            if prefix[0] & 0xF0 != 0x80 or prefix[1] & 128:
                raise ValueError("native_metadata_unsupported_frame")
            opcode, length = prefix[0] & 15, prefix[1] & 127
            if length == 126:
                length = int.from_bytes(read(2), "big")
            elif length == 127:
                length = int.from_bytes(read(8), "big")
            if length > 1024 * 1024 or (opcode >= 8 and length > 125):
                raise ValueError("native_metadata_limit")
            payload = read(length)
            if opcode == 9:
                send(payload, 10)
                continue
            if opcode != 1:
                raise ValueError("native_metadata_unsupported_frame")
            value = json.loads(payload)
            if not isinstance(value, dict):
                raise ValueError("native_metadata_protocol")
            return value

    def rpc(sequence, method, params):
        message({"id": sequence, "method": method, "params": params})
        while True:
            value = receive()
            if type(value.get("id")) is int and value["id"] == sequence:
                if "error" in value or not isinstance(value.get("result"), dict):
                    raise ValueError("native_metadata_rpc")
                return value["result"]
            # Notifications or requests belong to the native server. A metadata
            # observer must not answer another client's permission request.
            if "method" not in value:
                raise ValueError("native_metadata_protocol")

    try:
        connection.settimeout(min(.1, max(.001, timeout)))
        check()
        connection.connect(str(path))
        key = base64.b64encode(os.urandom(16)).decode("ascii")
        connection.sendall(("GET / HTTP/1.1\r\nHost: localhost\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n"
                            "Sec-WebSocket-Version: 13\r\nSec-WebSocket-Key: " + key + "\r\n\r\n").encode())
        header = bytearray()
        while not header.endswith(b"\r\n\r\n"):
            if len(header) >= 8192:
                raise ValueError("native_metadata_handshake_limit")
            header.extend(read(1))
        lines = header.decode("ascii").split("\r\n")
        fields = {}
        for line in lines[1:]:
            if line:
                name, value = line.split(":", 1)
                if name.lower() in fields:
                    raise ValueError("native_metadata_duplicate_header")
                fields[name.lower()] = value.strip()
        expected = base64.b64encode(hashlib.sha1((key + "258EAFA5-E914-47DA-95CA-C5AB0DC85B11").encode()).digest()).decode()
        if (not lines[0].startswith("HTTP/1.1 101 ") or fields.get("sec-websocket-accept") != expected
                or fields.get("upgrade", "").lower() != "websocket"
                or "upgrade" not in [v.strip().lower() for v in fields.get("connection", "").split(",")]):
            raise ValueError("native_metadata_handshake")
        rpc(1, "initialize", {"clientInfo": {"name": "coord_metadata", "version": "1"},
                              "capabilities": {"experimentalApi": True}})
        message({"method": "initialized"})
        result = rpc(2, "thread/read", {"threadId": thread_id, "includeTurns": False})
        thread = result.get("thread")
        if (not isinstance(thread, dict) or thread.get("id") != thread_id or not isinstance(thread.get("cwd"), str)
                or thread.get("canAcceptDirectInput") is not True):
            raise ValueError("native_metadata_identity")
        check()
        return {"id": thread_id, "cwd": thread["cwd"]}
    finally:
        connection.close()
