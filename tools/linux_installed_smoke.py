import functools
import http.server
from pathlib import Path
import subprocess
import sys
import tempfile
import threading


prefix = Path(sys.argv[1]).resolve()
compiler = prefix / "bin/aoxn"
handler = functools.partial(http.server.SimpleHTTPRequestHandler, directory=str(prefix / "examples"))
server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), handler)
worker = threading.Thread(target=server.serve_forever, daemon=True)
worker.start()
try:
    with tempfile.TemporaryDirectory(prefix="aoxn-installed-http-") as directory:
        subprocess.run(
            [str(compiler), "run", str(prefix / "examples/http_linux_probe.ax"), "--",
             f"http://127.0.0.1:{server.server_port}/hello.ax"],
            cwd=directory, check=True, timeout=120,
        )
finally:
    server.shutdown()
    server.server_close()
    worker.join(timeout=5)
