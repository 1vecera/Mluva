"""Own the loopback HTTP server used by provider and workflow contract tests."""

import threading
from collections.abc import Iterator
from contextlib import contextmanager
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer


@contextmanager
def local_http_server(handler: type[BaseHTTPRequestHandler]) -> Iterator[ThreadingHTTPServer]:
    """Serve real requests and always close the socket and thread, including on assertion failure."""
    with ThreadingHTTPServer(("127.0.0.1", 0), handler) as server:
        # Only shorten the idle shutdown poll; request deadlines remain unchanged.
        thread = threading.Thread(target=server.serve_forever, kwargs={"poll_interval": 0.01}, daemon=True)
        thread.start()
        try:
            yield server
        finally:
            server.shutdown()
            thread.join()
