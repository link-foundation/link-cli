"""Optional native IPC decorators for either end of a LiNo connection.

The public extension contract is Stream.recv/sendall/shutdown_write/close and
Listener.accept/close. No adapter interprets, retries or executes a LiNo query.
"""

from .endpoints import Options, connect, listen, register
from .streams import Stream, relay

__all__ = ["Options", "Stream", "connect", "listen", "register", "relay"]
