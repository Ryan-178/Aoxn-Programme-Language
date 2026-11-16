import functools
import http.server
import json
import os
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
        env = os.environ.copy()
        env.pop("AOXN_HOME", None)
        env.pop("AOXN_STDLIB", None)
        env["NO_PROXY"] = env["no_proxy"] = "*"
        probe = prefix / "examples/http_linux_probe.ax"
        result = subprocess.run(
            [str(compiler), "symbols", str(probe), "--json"],
            cwd=directory, env=env, check=True, capture_output=True, text=True, timeout=30,
        )
        symbols = json.loads(result.stdout)["symbols"]
        expected = (prefix / "lib/stdlib/net/http.ax").resolve()
        origins = {Path(symbol["file"]).resolve() for symbol in symbols
                   if symbol["name"] == "net_http_request"}
        if origins != {expected}:
            raise AssertionError(f"transport resolved outside installation: {origins}")
        subprocess.run(
            [str(compiler), "run", str(probe), "--",
             f"http://127.0.0.1:{server.server_port}/hello.ax"],
            cwd=directory, env=env, check=True, timeout=120,
        )
finally:
    server.shutdown()
    server.server_close()
    worker.join(timeout=5)
