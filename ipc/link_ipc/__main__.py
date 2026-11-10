"""clink-ipc --listen kind:address --connect kind:address [--once]."""

import argparse
import contextlib
import json
import os
import signal
import sys
import threading
from pathlib import Path

from .endpoints import Options, connect, listen, split_endpoint
from .local import enable_signals
from .streams import relay


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--listen", required=True, help="incoming endpoint; see ipc/README.md"
    )
    parser.add_argument("--connect", required=True, help="outgoing endpoint")
    parser.add_argument(
        "--once", action="store_true", help="serve one connection and exit"
    )
    parser.add_argument(
        "--timeout", type=float, default=30, help="setup/write stall timeout in seconds"
    )
    parser.add_argument(
        "--poll", type=float, default=0.01, help="mailbox poll interval in seconds"
    )
    parser.add_argument("--max-connections", type=int, default=16)
    parser.add_argument(
        "--trace", action="store_true", help="log byte counts and direction to stderr"
    )
    parser.add_argument(
        "--ready-file", type=Path, help="write the bound endpoint as JSON, exclusively"
    )
    args = parser.parse_args(argv)
    options = Options(args.timeout, args.poll)
    if not 1 <= args.max_connections <= 256:
        parser.error("--max-connections must be between 1 and 256")
    # Signal handlers are process-wide and must be installed on the main
    # thread, before a worker connects an outgoing signal endpoint.
    for endpoint in (args.listen, args.connect):
        if split_endpoint(endpoint)[0] == "signal":
            enable_signals()
    listener = listen(args.listen, options)
    stop, capacity = threading.Event(), threading.Semaphore(args.max_connections)
    workers, active, errors = [], set(), []
    lock = threading.Lock()

    def shutdown(*_):
        stop.set()
        listener.close()

    for name in ("SIGINT", "SIGTERM"):
        signal.signal(getattr(signal, name), shutdown)

    def trace(message):
        print(f"clink-ipc: {message}", file=sys.stderr, flush=True)

    def work(incoming):
        outgoing = None
        try:
            outgoing = connect(args.connect, options)
            with lock:
                active.add(outgoing)
            relay(incoming, outgoing, stop, trace if args.trace else None)
        except Exception as error:
            errors.append(str(error))
            trace(str(error))
        finally:
            incoming.close()
            if outgoing:
                outgoing.close()
            with lock:
                active.discard(incoming)
                active.discard(outgoing)
            capacity.release()

    try:
        if args.ready_file:
            fd = os.open(args.ready_file, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
            with os.fdopen(fd, "w") as output:
                json.dump({"endpoint": listener.address}, output)
        print(f"clink-ipc listening on {listener.address}", file=sys.stderr, flush=True)
        while not stop.is_set():
            if not capacity.acquire(timeout=0.2):
                continue
            try:
                incoming = listener.accept()
            except (OSError, ValueError, EOFError):
                capacity.release()
                if stop.is_set():
                    break
                raise
            with lock:
                active.add(incoming)
            worker = threading.Thread(target=work, args=(incoming,))
            worker.start()
            workers = [item for item in workers if item.is_alive()]
            workers.append(worker)
            if args.once or args.listen == "stdio:":
                break
    except BaseException:
        stop.set()
        raise
    finally:
        if stop.is_set():
            with lock:
                for stream in active:
                    with contextlib.suppress(OSError):
                        stream.close()
        for worker in workers:
            worker.join()
        listener.close()
    return int(bool(errors))


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (OSError, ValueError, EOFError) as error:
        print(f"clink-ipc: {error}", file=sys.stderr)
        sys.exit(1)
