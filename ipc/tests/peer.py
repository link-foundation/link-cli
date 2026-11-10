"""Independent-process native peer used by transport contract tests."""

import sys

from link_ipc import Options, listen

listener = listen(sys.argv[1], Options(timeout=3))
print(listener.address, flush=True)
try:
    with listener.accept() as stream:
        while data := stream.recv(997):
            stream.sendall(data)
        stream.shutdown_write()
finally:
    listener.close()
