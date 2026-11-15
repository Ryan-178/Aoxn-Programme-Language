//! stdlib/net.ax: URL parse/build, percent codecs, query lists (pure,
//! differential against CPython urllib.parse), and the Windows HTTP facade
//! against a local Rust mock server.

use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};
#[cfg(windows)]
use std::{
    io::{Read, Write},
    net::TcpListener,
};

const EXE: &str = if cfg!(windows) { ".exe" } else { "" };

fn libs() -> Vec<String> {
    if cfg!(windows) {
        vec!["winhttp".to_string()]
    } else {
        vec![]
    }
}

const SRC: &str = r#"
import * from "stdlib"
import * from "stdlib/net"

def fld(s: string) -> string:
    if len(s) == 0:
        return "-"
    return s

def show(u: Url):
    print(u.ok, fld(u.scheme), fld(u.host), u.port, fld(u.path), fld(u.query), fld(u.fragment))

def main() -> int:
    show(url_parse("https://example.com:8443/a/b?x=1&y=two%20words#frag"))
    show(url_parse("http://localhost/path"))
    show(url_parse("http://h"))
    show(url_parse("https://h?#f"))
    show(url_parse("http://[::1]:8080/p"))
    show(url_parse("ftp://files.example.org/pub"))
    show(url_parse("http://user:pass@h/"))
    show(url_parse("no-scheme"))
    show(url_parse("http://h:99999/"))
    show(url_parse("http://h:abc/"))
    show(url_parse("http:///path"))
    u = url_parse("https://example.com:8443/a/b?x=1#f")
    print("build", url_build(u))
    v = url_parse("http://[::1]/p")
    print("build6", url_build(v))
    print("quote", url_quote("a b/c?d"))
    print("quote-safe", url_quote_safe("a b/c", ""))
    print("plus", url_quote_plus("a b/c"))
    print("unquote", url_unquote("a%20b%2Fc%zz"))
    print("unquote-plus", url_unquote_plus("a+b%2Bc"))
    q = url_query_parse("name=aoxn+lang&tags=a%2Cb&empty=&flag")
    print("pairs", q.n)
    i = 0
    while i < q.n:
        print("pair", fld(vec_get_str(q.keys, i)), fld(vec_get_str(q.vals, i)))
        i = i + 1
    ks = vec_new()
    vs = vec_new()
    ks = vec_push_str(ks, "a b")
    vs = vec_push_str(vs, "c&d")
    ks = vec_push_str(ks, "e")
    vs = vec_push_str(vs, "")
    print("encode", url_query_encode(ks, vs))
    print("DONE")
    return 0
"#;

const DIFF_SRC: &str = r#"
import * from "stdlib"
import * from "stdlib/net"

def qs(s: string):
    print("Q", url_quote(s))
    print("P", url_quote_plus(s))

def us(s: string):
    print("U", url_unquote(s))
    print("UP", url_unquote_plus(s))

def show(u: Url):
    print("S", u.ok, u.scheme, u.host, u.port, u.path, u.query, u.fragment)

def main() -> int:
    qs("a b/c?d")
    qs("hello~world_-.")
    qs("100% sure")
    qs("a=b&c=d")
    qs("/path/to/x")
    qs("")
    p = malloc(3)
    store_u8(p, 0, 195)
    store_u8(p, 1, 169)
    store_u8(p, 2, 0)
    qs(as_string(p))
    us("a%20b%2Fc")
    us("%zz%2")
    us("100%25")
    us("%C3%A9")
    us("a+b")
    us("")
    us("%c3%a9+x")
    show(url_parse("https://example.com:8443/a/b?x=1&y=two%20words#frag"))
    show(url_parse("http://localhost/path"))
    show(url_parse("http://h"))
    show(url_parse("https://h?#f"))
    show(url_parse("http://[::1]:8080/p"))
    show(url_parse("ftp://files.example.org/pub"))
    show(url_parse("no-scheme"))
    show(url_parse("http://h:99999/"))
    show(url_parse("http://h:abc/"))
    print("DONE")
    return 0
"#;

const PY: &str = r#"
import urllib.parse as up

def qs(s):
    print("Q", up.quote(s, safe="/"))
    print("P", up.quote_plus(s))

def us(s):
    print("U", up.unquote(s))
    print("UP", up.unquote_plus(s))

def show(s):
    try:
        p = up.urlsplit(s)
        port = p.port
    except ValueError:
        print("S", 0, "", "", 0, "", "", "")
        return
    if port is None:
        port = {"http": 80, "https": 443}.get(p.scheme, 0)
    path = p.path or "/"
    ok = 1 if p.scheme and p.hostname else 0
    print("S", ok, p.scheme, p.hostname or "", port, path, p.query, p.fragment)

qs("a b/c?d")
qs("hello~world_-.")
qs("100% sure")
qs("a=b&c=d")
qs("/path/to/x")
qs("")
qs("\u00e9")
us("a%20b%2Fc")
us("%zz%2")
us("100%25")
us("%C3%A9")
us("a+b")
us("")
us("%c3%a9+x")
show("https://example.com:8443/a/b?x=1&y=two%20words#frag")
show("http://localhost/path")
show("http://h")
show("https://h?#f")
show("http://[::1]:8080/p")
show("ftp://files.example.org/pub")
show("no-scheme")
show("http://h:99999/")
show("http://h:abc/")
print("DONE")
"#;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn build(dir: &Path, name: &str, src: &str) -> PathBuf {
    let src_path = dir.join(format!("{name}.ax"));
    fs::write(&src_path, src).unwrap();
    let exe = dir.join(format!("{name}{EXE}"));
    aoxn::build_paths_opts(&[src_path.display().to_string()], &exe, true, &libs(), &[])
        .unwrap_or_else(|d| panic!("{name} failed: {d:?}"));
    exe
}

#[test]
fn url_layer_parse_codecs_and_queries() {
    let dir = root().join("target").join(format!("net-test-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let exe = build(&dir, "net_driver", SRC);
    let out = Command::new(&exe).output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let expected = "\
1 https example.com 8443 /a/b x=1&y=two%20words frag
1 http localhost 80 /path - -
1 http h 80 / - -
1 https h 443 / - f
1 http ::1 8080 /p - -
1 ftp files.example.org 0 /pub - -
0 - - 0 - - -
0 - - 0 no-scheme - -
0 - - 0 - - -
0 - - 0 - - -
0 - - 0 - - -
build https://example.com:8443/a/b?x=1#f
build6 http://[::1]/p
quote a%20b/c%3Fd
quote-safe a%20b%2Fc
plus a+b%2Fc
unquote a b/c%zz
unquote-plus a b+c
pairs 4
pair name aoxn lang
pair tags a,b
pair empty -
pair flag -
encode a+b=c%26d&e=
DONE
";
    assert_eq!(String::from_utf8_lossy(&out.stdout), expected);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn url_layer_matches_python() {
    let py = Command::new("python").arg("--version").output();
    let Ok(py) = py else {
        eprintln!("skipping: python unavailable");
        return;
    };
    if !py.status.success() {
        eprintln!("skipping: python unavailable");
        return;
    }
    let dir = root().join("target").join(format!("net-diff-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let exe = build(&dir, "net_diff", DIFF_SRC);
    let out = Command::new(&exe).output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let pyfile = dir.join("diff.py");
    fs::write(&pyfile, PY).unwrap();
    let pyout = Command::new("python")
        .arg(&pyfile)
        .env("PYTHONUTF8", "1")
        .env("PYTHONIOENCODING", "utf-8")
        .output()
        .unwrap();
    assert!(pyout.status.success(), "{}", String::from_utf8_lossy(&pyout.stderr));
    // CPython's Windows stdout is text-mode CRLF; the Aoxn driver emits LF.
    let norm = |b: &[u8]| -> Vec<u8> { b.iter().copied().filter(|&c| c != b'\r').collect() };
    assert_eq!(norm(&out.stdout), norm(&pyout.stdout));
    fs::remove_dir_all(&dir).unwrap();
}

#[cfg(windows)]
#[test]
fn http_facade_against_local_server() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    // Serve FOREVER, not a fixed request count: WinHTTP may open extra
    // connections (proxy autodiscovery, retries), and a mock that stops
    // accepting after N turns a surplus connection into a driver hang —
    // which then deadlocks `server.join()` below. The listener dies with
    // the test process; the driver is bounded by the timeout instead.
    let _server = std::thread::spawn(move || loop {
        let Ok((mut sock, _)) = listener.accept() else {
            return;
        };
        let mut buf = Vec::new();
        let mut tmp = [0u8; 4096];
        // WinHTTP opens probe connections that connect and close without
        // sending a byte; they must NOT consume a request slot — skip
        // them and keep the connection open until a real request arrives.
        loop {
            let Ok(n) = sock.read(&mut tmp) else {
                break;
            };
            if n == 0 {
                break;
            }
            buf.extend_from_slice(&tmp[..n]);
            if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                let header_end = pos + 4;
                let head = String::from_utf8_lossy(&buf[..header_end]).into_owned();
                let mut body = Vec::new();
                if head.starts_with("POST") {
                    let clen: usize = head
                        .lines()
                        .find_map(|l| {
                            let (k, v) = l.split_once(':')?;
                            if k.trim().eq_ignore_ascii_case("content-length") {
                                v.trim().parse().ok()
                            } else {
                                None
                            }
                        })
                        .unwrap_or(0);
                    body.extend_from_slice(&buf[header_end..]);
                    while body.len() < clen {
                        let Ok(n) = sock.read(&mut tmp) else {
                            return;
                        };
                        if n == 0 {
                            return;
                        }
                        body.extend_from_slice(&tmp[..n]);
                    }
                }
                let (status, payload): (&str, Vec<u8>) = if head.starts_with("GET /hello") {
                    ("200 OK", b"hello-net".to_vec())
                } else if head.starts_with("POST /echo") {
                    ("200 OK", body)
                } else {
                    ("404 Not Found", b"nope".to_vec())
                };
                let resp = format!(
                    "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    payload.len()
                );
                let _ = sock.write_all(resp.as_bytes());
                let _ = sock.write_all(&payload);
                let _ = sock.flush();
                break;
            }
        }
    });

    let dir = root().join("target").join(format!("net-http-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let src = r#"
import * from "stdlib"
import * from "stdlib/net"

def main() -> int:
    r = http_get("http://127.0.0.1:__PORT__/hello")
    print("status", r.status)
    print("body", r.body)
    print("ok", http_ok(r))
    p = http_post("http://127.0.0.1:__PORT__/echo", "payload=42", "application/x-www-form-urlencoded")
    print("post-status", p.status)
    print("post-body", p.body)
    m = http_get("http://127.0.0.1:__PORT__/missing")
    print("missing", m.status, http_ok(m))
    print("DONE")
    return 0
"#
    .replace("__PORT__", &port.to_string());
    let exe = build(&dir, "net_http", &src);
    // bounded wait: a transport stall must fail the test, not hang it
    let child = Command::new(&exe)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let pid = child.id();
    let (tx, rx) = std::sync::mpsc::channel();
    let waiter = std::thread::spawn(move || {
        let out = child.wait_with_output();
        let _ = tx.send(out);
    });
    let out = match rx.recv_timeout(std::time::Duration::from_secs(90)) {
        Ok(Ok(out)) => out,
        Ok(Err(e)) => panic!("driver wait failed: {e}"),
        Err(_) => {
            // kill by pid: the Child itself lives in the waiter thread
            let _ = Command::new("taskkill")
                .args(["/PID", &pid.to_string(), "/F"])
                .output();
            panic!("http facade driver did not finish within 90 s (transport stall)");
        }
    };
    let _ = waiter.join();
    // the server thread is detached on purpose: it accepts forever, so
    // joining it would hang; it dies with the test process
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let expected = "\
status 200
body hello-net
ok true
post-status 200
post-body payload=42
missing 404 false
DONE
";
    assert_eq!(String::from_utf8_lossy(&out.stdout), expected);
    fs::remove_dir_all(&dir).unwrap();
}