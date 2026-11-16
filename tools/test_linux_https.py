"""Native Linux HTTPS untrusted-certificate rejection only.

Run with python -B tools/test_linux_https.py after building target/debug/aoxn.
AOXN_TEST_COMPILER may select another compiler. Requires openssl, clang, rustc,
and the Linux libcurl development files. This does not test trusted certificates
or successful hostname verification, and never disables certificate validation.
"""

import http.server
import os
from pathlib import Path
import shutil
import ssl
import subprocess
import sys
import tempfile
import threading
import unittest


ROOT = Path(__file__).resolve().parents[1]


def client_source() -> str:
    """Keep source generation usable for typechecking on non-Linux hosts."""
    provider = (ROOT / "stdlib/net/http_linux.ax").as_posix()
    transport = (ROOT / "stdlib/net/http.ax").as_posix()
    return f'''import * from "{provider}"
import * from "{transport}"

def main() -> int:
    url = arg(0)
    response = net_http_request("GET", url, "", "", "aoxn-linux-https-test", 3000, 5000)
    assert(response.err == 1, "request accepted an untrusted certificate")
    assert(len(response.errmsg) > 0, "request omitted its transport error")
    print("REQUEST_ERROR " + response.errmsg)
    stream = net_http_stream_open("GET", url, "", "", "aoxn-linux-https-test", 3000, 5000)
    net_http_stream_close(stream)
    assert(stream.err == 1, "stream accepted an untrusted certificate")
    assert(len(stream.errmsg) > 0, "stream omitted its transport error")
    print("STREAM_ERROR " + stream.errmsg)
    return 0
'''


class LocalHandler(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        self.send_response(200)
        self.send_header("Content-Length", "2")
        self.end_headers()
        self.wfile.write(b"ok")

    def log_message(self, format, *args):
        pass


@unittest.skipUnless(sys.platform.startswith("linux"), "requires native Linux")
class LinuxHttpsTests(unittest.TestCase):
    def test_request_and_stream_reject_untrusted_certificate(self):
        openssl = shutil.which("openssl")
        self.assertIsNotNone(openssl, "openssl is required for this integration test")
        compiler = Path(os.environ.get("AOXN_TEST_COMPILER", str(ROOT / "target/debug/aoxn"))).resolve()
        self.assertTrue(compiler.is_file(), f"build or select the Aoxn compiler: {compiler}")
        env = os.environ.copy()
        env["NO_PROXY"] = env["no_proxy"] = "localhost,127.0.0.1,::1"
        # No external proxy or user-supplied trust-store override belongs in this probe.
        for name in list(env):
            if name.lower() in {"http_proxy", "https_proxy", "all_proxy"} or name in {
                "CURL_CA_BUNDLE", "SSL_CERT_FILE", "SSL_CERT_DIR",
            }:
                env.pop(name)

        with tempfile.TemporaryDirectory(prefix="aoxn-linux-https-") as directory:
            work = Path(directory)
            cert = work / "localhost.pem"
            key = work / "localhost.key"
            generated = subprocess.run(
                [openssl, "req", "-x509", "-newkey", "rsa:2048", "-nodes",
                 "-keyout", str(key), "-out", str(cert), "-days", "1",
                 "-subj", "/CN=localhost", "-addext", "subjectAltName=DNS:localhost"],
                cwd=work, env=env, capture_output=True, text=True, timeout=30,
            )
            self.assertEqual(generated.returncode, 0, generated.stdout + generated.stderr)
            source = work / "reject_untrusted.ax"
            source.write_text(client_source(), encoding="utf-8")
            executable = work / "reject_untrusted"
            compiled = subprocess.run(
                [str(compiler), "build", str(source), "-o", str(executable)],
                cwd=work, env=env, capture_output=True, text=True, timeout=180,
            )
            self.assertEqual(compiled.returncode, 0, compiled.stdout + compiled.stderr)

            context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
            context.load_cert_chain(certfile=cert, keyfile=key)
            server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), LocalHandler)
            worker = None
            try:
                server.socket = context.wrap_socket(server.socket, server_side=True)
                worker = threading.Thread(
                    target=server.serve_forever, kwargs={"poll_interval": 0.05}, daemon=True,
                )
                worker.start()
                result = subprocess.run(
                    [str(executable), f"https://localhost:{server.server_port}/"],
                    cwd=work, env=env, capture_output=True, text=True, timeout=30,
                )
                self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
                for marker in ("REQUEST_ERROR ", "STREAM_ERROR "):
                    with self.subTest(api=marker.strip()):
                        errors = [line[len(marker):] for line in result.stdout.splitlines()
                                  if line.startswith(marker)]
                        self.assertEqual(len(errors), 1, result.stdout + result.stderr)
                        # A generic connection failure must not masquerade as TLS rejection.
                        self.assertTrue(
                            any(word in errors[0].lower() for word in
                                ("certificate", "self-signed", "ssl peer")),
                            f"expected a certificate verification error, got: {errors[0]}",
                        )
            finally:
                if worker is not None and worker.is_alive():
                    server.shutdown()
                server.server_close()
                if worker is not None:
                    worker.join(timeout=5)
                    self.assertFalse(worker.is_alive(), "HTTPS server did not stop")


if __name__ == "__main__":
    unittest.main()
