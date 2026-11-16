//! stdlib/net.ax: URL parse/build, percent codecs, query lists (pure,
//! differential against CPython urllib.parse), and the Windows HTTP facade
//! against a local Rust mock server.

use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};
#[cfg(any(windows, target_os = "linux", target_os = "macos"))]
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
    // The provider is opt-in: Windows keeps WinHTTP without a Rust bridge.
    let body = if cfg!(target_os = "macos") {
        format!("import * from \"stdlib/net/http_macos\"\n{src}")
    } else if cfg!(target_os = "linux") {
        format!("import * from \"stdlib/net/http_linux\"\n{src}")
    } else {
        src.to_string()
    };
    // Pin fixtures to this checkout, not an installed AOXN_HOME/AOXN_STDLIB.
    let stdlib = root().join("stdlib").to_string_lossy().replace('\\', "/");
    let body = body
        .replace("\"stdlib\"", &format!("\"{stdlib}/stdlib.ax\""))
        .replace("\"stdlib/net\"", &format!("\"{stdlib}/net.ax\""))
        .replace("\"stdlib/net/http_macos\"", &format!("\"{stdlib}/net/http_macos.ax\""))
        .replace("\"stdlib/net/http_linux\"", &format!("\"{stdlib}/net/http_linux.ax\""));
    fs::write(&src_path, body.as_bytes()).unwrap();
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

#[cfg(any(windows, target_os = "linux", target_os = "macos"))]
#[test]
fn http_facade_against_local_server() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    // Deadline-bounded listener and socket I/O, including probe connections.
    listener.set_nonblocking(true).unwrap();
    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let server_stop = stop.clone();
    let redirected = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let server_redirected = redirected.clone();
    let server = std::thread::spawn(move || {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(180);
        while !server_stop.load(std::sync::atomic::Ordering::Relaxed)
            && std::time::Instant::now() < deadline
        {
        let (mut sock, _) = match listener.accept() {
            Ok(s) => s,
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(std::time::Duration::from_millis(10));
                continue;
            }
            Err(e) => panic!("accept: {e}"),
        };
        sock.set_read_timeout(Some(std::time::Duration::from_secs(3))).unwrap();
        sock.set_write_timeout(Some(std::time::Duration::from_secs(3))).unwrap();
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
                if head.starts_with("GET /target") {
                    server_redirected.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                }
                if head.starts_with("GET /redirect") {
                    let _ = sock.write_all(b"HTTP/1.1 302 Found\r\nLocation: /target\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
                    break;
                }
                if head.starts_with("GET /truncated") {
                    let _ = sock.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 100\r\nConnection: close\r\n\r\nshort");
                    break;
                }
                // Streaming must not return the interim response's status/headers.
                if head.starts_with("POST /stream") {
                    let _ = sock.write_all(b"HTTP/1.1 100 Continue\r\nrequest-id: interim\r\n\r\n");
                }
                let (status, payload): (&str, Vec<u8>) = if head.starts_with("GET /hello") {
                    ("200 OK", b"hello-net".to_vec())
                } else if head.starts_with("POST /binary") || head.starts_with("POST /stream") {
                    assert_eq!(body, b"a\0b");
                    ("200 OK", b"data: one\n\ndata: two\n\n".to_vec())
                } else if head.starts_with("POST /echo") {
                    ("200 OK", body)
                } else {
                    ("404 Not Found", b"nope".to_vec())
                };
                let resp = format!(
                    "HTTP/1.1 {status}\r\nrequest-id: final\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    payload.len()
                );
                let _ = sock.write_all(resp.as_bytes());
                let _ = sock.write_all(&payload);
                let _ = sock.flush();
                break;
            }
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
    post = http_post("http://127.0.0.1:__PORT__/echo", "payload=42", "application/x-www-form-urlencoded")
    print("post-status", post.status)
    print("post-body", post.body)
    m = http_get("http://127.0.0.1:__PORT__/missing")
    print("missing", m.status, http_ok(m))
    p = malloc(3)
    store_u8(p, 0, 97)
    store_u8(p, 1, 0)
    store_u8(p, 2, 98)
    b = net_http_request_bytes("POST", "http://127.0.0.1:__PORT__/binary", "", p, 3, "net-test", 2000, 5000)
    print("binary", b.status, b.err, b.body == "data: one\n\ndata: two\n\n")
    s = net_http_stream_open_bytes("POST", "http://127.0.0.1:__PORT__/stream", "", p, 3, "net-test", 2000, 5000)
    free(p)
    print("stream-open", s.status, s.err, s.request_id)
    buf = malloc(5)
    text = sb_new(32)
    while s.err == 0:
        s = net_http_stream_read(s, buf, 5)
        if s.got == 0:
            break
        i = 0
        while i < s.got:
            text = sb_byte(text, load_u8(buf, i))
            i = i + 1
    print("stream-body", s.err, sb_text(text) == "data: one\n\ndata: two\n\n")
    net_http_stream_close(s)
    free(buf)
    redir = net_http_request("GET", "http://127.0.0.1:__PORT__/redirect", "Authorization: secret\r\n", "", "net-test", 2000, 5000)
    print("redirect", redir.status, redir.err, redir.location)
    t = net_http_request("GET", "http://127.0.0.1:__PORT__/truncated", "", "", "net-test", 2000, 5000)
    print("truncated", t.err, len(t.errmsg) > 0)
    t = net_http_request("GET", "file:///not-an-http-resource", "", "", "net-test", 2000, 5000)
    print("scheme", t.err, len(t.errmsg) > 0)
    s = net_http_stream_open("GET", "ftp://127.0.0.1:__PORT__/", "", "", "net-test", 2000, 5000)
    print("stream-scheme", s.err, len(s.errmsg) > 0)
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
            #[cfg(windows)]
            let _ = Command::new("taskkill")
                .args(["/PID", &pid.to_string(), "/F"])
                .output();
            #[cfg(not(windows))]
            let _ = Command::new("kill").args(["-KILL", &pid.to_string()]).output();
            panic!("http facade driver did not finish within 90 s (transport stall)");
        }
    };
    let _ = waiter.join();
    stop.store(true, std::sync::atomic::Ordering::Relaxed);
    server.join().unwrap();
    assert_eq!(redirected.load(std::sync::atomic::Ordering::Relaxed), 0, "redirect was followed");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let expected = "\
status 200
body hello-net
ok true
post-status 200
post-body payload=42
missing 404 false
binary 200 0 true
stream-open 200 0 final
stream-body 0 true
redirect 302 0 /target
truncated 1 true
scheme 1 true
stream-scheme 1 true
DONE
";
    assert_eq!(String::from_utf8_lossy(&out.stdout), expected);
    fs::remove_dir_all(&dir).unwrap();
}