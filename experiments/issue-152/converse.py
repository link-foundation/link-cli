#!/usr/bin/env python3
"""Plays requests against a running `clink --serve --triplets` in the text protocol.

usage: converse.py HOST:PORT < requests, each line `A> request`; prints each
reply and any push the other connection received, to draft or check
docs/protocol/triplets.txt against either port.
"""
import select
import socket
import sys

host, port = sys.argv[1].rsplit(":", 1)
connections = {}


buffers = {}


def read_document(name, timeout):
    sock = connections[name]
    buffer = buffers.get(name, b"")
    while b"\n.\n" not in buffer and not buffer.startswith(b".\n"):
        ready, _, _ = select.select([sock], [], [], timeout)
        if not ready:
            buffers[name] = buffer
            return None
        chunk = sock.recv(65536)
        if not chunk:
            break
        buffer += chunk
    if buffer.startswith(b".\n"):
        buffers[name] = buffer[2:]
        return ""
    document, _, rest = buffer.partition(b"\n.\n")
    buffers[name] = rest
    return document.decode()


for line in sys.stdin:
    line = line.rstrip("\n")
    if not line or line.startswith("#"):
        print(line)
        continue
    name, request = line[0], line[3:]
    print(line)
    socket_of = connections.get(name) or socket.create_connection((host, int(port)))
    connections[name] = socket_of
    socket_of.sendall(request.encode() + b"\n.\n")
    while True:
        document = read_document(name, 5)
        if document is None:
            print("! no reply")
            break
        if document.startswith("(notify:"):
            print(f"{name}< {document}")
            continue
        if document:
            print(document)
        break
    for other in connections:
        if other != name:
            while True:
                push = read_document(other, 0.1)
                if push is None:
                    break
                print(f"{other}< {push}")
