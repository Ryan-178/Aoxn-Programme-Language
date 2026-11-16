#![cfg(target_os = "linux")]

//! Actual SDK -> Linux provider -> libcurl -> loopback HTTP round trips.
//! No external credentials/network; each SDK gets its own Aoxn driver.
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const OPENAI: &str = r#"
def main() -> int:
    assert(net_http_linux_ready())
    c = oa_client_at("fake-linux-key", arg(0) + "/v1")
    c.connect_ms = 1000
    c.timeout_ms = 3000
    c.max_retries = 1
    roles = vec_push_str(vec_new(), "user")
    contents = vec_push_str(vec_new(), "linux sdk probe")
    r = oa_chat_create(c, "mock-model", roles, contents)
    assert(oa_resp_ok(r), oa_err_msg(r))
    assert(r.retries == 1)
    assert(r.request_id == "linux-request")
    assert(oa_chat_text(r) == "Linux completion")
    assert(oa_chat_finish(r) == "stop")
    s = oa_chat_stream(c, "mock-model", roles, contents, 0.0, 0, 32)
    assert(s.status == 200 and s.err == 0, s.errmsg)
    text = ""
    finish = ""
    events = 0
    while not s.done and s.err == 0 and events < 16:
        s = oa_stream_next(s)
        if s.res != "":
            text = text + oa_stream_text(s.res)
            finish = finish + oa_stream_finish(s.res)
        events = events + 1
    oa_stream_close(s)
    assert(s.done and s.err == 0, s.errmsg)
    assert(text == "Linux stream")
    assert(finish == "stop")
    print("PASS linux-openai retry completion stream")
    return 0
"#;

const ANTHROPIC: &str = r#"
def main() -> int:
    assert(net_http_linux_ready())
    c = an_client_at("fake-linux-key", arg(0))
    c.connect_ms = 1000
    c.timeout_ms = 3000
    c.max_retries = 1
    msgs = an_msgs_push_text(an_msgs_new(), "user", "linux sdk probe")
    opts = an_opts_new()
    r = an_messages_create(c, "mock-model", msgs, 32, opts)
    assert(an_resp_ok(r), an_err_msg(r))
    assert(r.retries == 1)
    assert(r.request_id == "linux-request")
    assert(an_msg_text(r) == "Linux completion")
    s = an_messages_stream(c, "mock-model", msgs, 32, opts)
    assert(s.status == 200 and s.err == 0, s.errmsg)
    text = ""
    named = False
    events = 0
    while not s.done and s.err == 0 and events < 16:
        s = an_stream_next(s)
        if s.res != "":
            text = text + an_stream_text(s.res)
        if s.ev == "content_block_delta":
            named = True
        events = events + 1
    an_stream_close(s)
    assert(s.done and s.err == 0, s.errmsg)
    assert(named)
    assert(text == "Linux stream")
    assembled = an_acc_resp(s.acc)
    assert(an_msg_text(assembled) == "Linux stream")
    print("PASS linux-anthropic retry completion stream")
    return 0
"#;

fn replies(anthropic: bool) -> (String, String) {
    if anthropic {
        let completion = r#"{"id":"msg_linux","type":"message","role":"assistant","model":"mock-model","content":[{"type":"text","text":"Linux completion"}],"stop_reason":"end_turn","usage":{"input_tokens":3,"output_tokens":2}}"#;
        let events = [
            (
                "message_start",
                r#"{"type":"message_start","message":{"id":"msg_stream","type":"message","role":"assistant","model":"mock-model","content":[],"usage":{"input_tokens":3,"output_tokens":0}}}"#,
            ),
            (
                "content_block_start",
                r#"{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}"#,
            ),
            (
                "content_block_delta",
                r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Linux "}}"#,
            ),
            (
                "content_block_delta",
                r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"stream"}}"#,
            ),
            (
                "content_block_stop",
                r#"{"type":"content_block_stop","index":0}"#,
            ),
            (
                "message_delta",
                r#"{"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":2}}"#,
            ),
            ("message_stop", r#"{"type":"message_stop"}"#),
        ];
        (
            completion.into(),
            events
                .iter()
                .map(|(event, data)| format!("event: {event}\ndata: {data}\n\n"))
                .collect(),
        )
    } else {
        (r#"{"id":"chat_linux","model":"mock-model","choices":[{"message":{"role":"assistant","content":"Linux completion"},"finish_reason":"stop"}]}"#.into(),
         concat!(
             "data: {\"choices\":[{\"delta\":{\"content\":\"Linux \"},\"finish_reason\":null}]}\n\n",
             "data: {\"choices\":[{\"delta\":{\"content\":\"stream\"},\"finish_reason\":null}]}\n\n",
             "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n",
             "data: [DONE]\n\n").into())
    }
}

fn request(stream: &mut TcpStream) -> Result<(String, String), String> {
    stream
        .set_read_timeout(Some(Duration::from_millis(200)))
        .map_err(|e| e.to_string())?;
    stream
        .set_write_timeout(Some(Duration::from_secs(2)))
        .map_err(|e| e.to_string())?;
    let deadline = Instant::now() + Duration::from_secs(4);
    let mut bytes = Vec::new();
    loop {
        if Instant::now() >= deadline || bytes.len() > 65536 {
            return Err("request exceeded time/size bound".into());
        }
        if let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
            let headers = String::from_utf8(bytes[..end].to_vec()).map_err(|e| e.to_string())?;
            let length = headers
                .lines()
                .skip(1)
                .find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse::<usize>())
                })
                .ok_or("missing Content-Length")?
                .map_err(|e| e.to_string())?;
            if length > 32768 {
                return Err("oversized body".into());
            }
            if bytes.len() >= end + 4 + length {
                let body = String::from_utf8(bytes[end + 4..end + 4 + length].to_vec())
                    .map_err(|e| e.to_string())?;
                return Ok((headers, body));
            }
        }
        let mut buf = [0; 4096];
        match stream.read(&mut buf) {
            Ok(0) => return Err("EOF inside request".into()),
            Ok(n) => bytes.extend_from_slice(&buf[..n]),
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) => {}
            Err(e) => return Err(e.to_string()),
        }
    }
}

struct Server {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<Result<(), String>>>,
}
impl Server {
    fn start(listener: TcpListener, anthropic: bool) -> Self {
        listener.set_nonblocking(true).unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let cancelled = Arc::clone(&stop);
        let thread = thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(20);
            let (completion, sse) = replies(anthropic);
            let mut previous_body = String::new();
            for index in 0..3 {
                let mut stream = loop {
                    if cancelled.load(Ordering::Relaxed) || Instant::now() >= deadline {
                        return Err(format!("server cancelled/timed out before request {index}"));
                    }
                    match listener.accept() {
                        Ok((stream, _)) => break stream,
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(10))
                        }
                        Err(e) => return Err(e.to_string()),
                    }
                };
                let (headers, body) = request(&mut stream)?;
                let path = if anthropic {
                    "/v1/messages"
                } else {
                    "/v1/chat/completions"
                };
                if headers.lines().next() != Some(&format!("POST {path} HTTP/1.1")) {
                    return Err(format!("unexpected request line: {headers}"));
                }
                let header = |name: &str| -> Option<&str> {
                    headers.lines().skip(1).find_map(|line| {
                        let (key, value) = line.split_once(':')?;
                        key.eq_ignore_ascii_case(name).then_some(value.trim())
                    })
                };
                let auth = if anthropic {
                    header("x-api-key") == Some("fake-linux-key")
                        && header("anthropic-version") == Some("2023-06-01")
                } else {
                    header("authorization") == Some("Bearer fake-linux-key")
                };
                if !auth || header("content-type") != Some("application/json") {
                    return Err(format!("missing SDK headers: {headers}"));
                }
                let agent = if anthropic {
                    "aoxn-anthropic/"
                } else {
                    "aoxn-openai/"
                };
                if !header("user-agent").unwrap_or("").starts_with(agent) {
                    return Err(format!("wrong user-agent: {headers}"));
                }
                if anthropic
                    && header("accept")
                        != Some(if index == 2 {
                            "text/event-stream"
                        } else {
                            "application/json"
                        })
                {
                    return Err(format!("wrong Anthropic Accept: {headers}"));
                }
                let compact: String = body.chars().filter(|c| !c.is_ascii_whitespace()).collect();
                // Both SDKs omit stream entirely for ordinary JSON requests.
                let stream_flag_ok = if index == 2 {
                    compact.contains("\"stream\":true")
                } else {
                    !compact.contains("\"stream\":")
                };
                if !compact.contains("\"model\":\"mock-model\"")
                    || !body.contains("linux sdk probe")
                    || !stream_flag_ok
                {
                    return Err(format!("wrong SDK body: {body}"));
                }
                if index == 1 && body != previous_body {
                    return Err("retry changed request body".into());
                }
                previous_body = body;
                let (status, kind, payload) = match index {
                    0 => (
                        "429 Too Many Requests",
                        "application/json",
                        "{\"error\":{\"type\":\"rate_limit_error\",\"message\":\"retry locally\"}}",
                    ),
                    1 => ("200 OK", "application/json", completion.as_str()),
                    _ => ("200 OK", "text/event-stream", sse.as_str()),
                };
                // Both SDKs' request-id spelling; a one-second Retry-After keeps retries bounded.
                write!(stream, "HTTP/1.1 {status}\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nx-request-id: linux-request\r\nrequest-id: linux-request\r\nRetry-After: 1\r\nConnection: close\r\n\r\n", payload.len()).map_err(|e| e.to_string())?;
                // Split writes exercise the transport read loop without assuming TCP packet boundaries.
                for chunk in payload.as_bytes().chunks(37) {
                    stream.write_all(chunk).map_err(|e| e.to_string())?;
                }
            }
            Ok(())
        });
        Self {
            stop,
            thread: Some(thread),
        }
    }
    fn finish(mut self) -> Result<(), String> {
        self.thread
            .take()
            .unwrap()
            .join()
            .map_err(|_| "server panicked".to_string())?
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

struct Process(Child);
impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
struct Scratch(PathBuf);
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn import(path: &str) -> String {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(path)
        .display()
        .to_string()
        .replace('\\', "/")
}

fn roundtrip(anthropic: bool) {
    // Missing toolchains/libcurl are failures, not silently green skipped coverage.
    assert!(
        aoxn::find_clang().is_some(),
        "Linux SDK tests require clang"
    );
    let sdk = if anthropic { "anthropic" } else { "openai" };
    let scratch =
        Scratch(std::env::temp_dir().join(format!("aoxn-linux-sdk-{sdk}-{}", std::process::id())));
    std::fs::create_dir_all(&scratch.0).unwrap();
    let source = scratch.0.join("driver.ax");
    let exe = scratch.0.join("driver");
    std::fs::write(
        &source,
        format!(
            "import * from \"{}\"\nimport * from \"{}\"\n{}",
            import("stdlib/net/http_linux.ax"),
            import(&format!("stdlib/{sdk}/client.ax")),
            if anthropic { ANTHROPIC } else { OPENAI }
        ),
    )
    .unwrap();
    aoxn::build_paths_opts(&[source.display().to_string()], &exe, true, &[], &[])
        .unwrap_or_else(|d| panic!("Linux {sdk} driver build failed (requires rustc and libcurl development files): {d:?}"));
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let base_url = format!("http://{}", listener.local_addr().unwrap());
    let server = Server::start(listener, anthropic);
    // Files avoid stdout/stderr pipe-buffer deadlocks while waiting with a deadline.
    let stdout = scratch.0.join("stdout");
    let stderr = scratch.0.join("stderr");
    let mut child = Process(
        Command::new(&exe)
            .arg(base_url)
            .env("NO_PROXY", "*")
            .env("no_proxy", "*")
            .stdout(Stdio::from(std::fs::File::create(&stdout).unwrap()))
            .stderr(Stdio::from(std::fs::File::create(&stderr).unwrap()))
            .spawn()
            .unwrap(),
    );
    let deadline = Instant::now() + Duration::from_secs(20);
    let status = loop {
        if let Some(status) = child.0.try_wait().unwrap() {
            break status;
        }
        if Instant::now() >= deadline {
            child.0.kill().unwrap();
            child.0.wait().unwrap();
            panic!(
                "{sdk} driver timed out\n{}\n{}",
                std::fs::read_to_string(&stdout).unwrap(),
                std::fs::read_to_string(&stderr).unwrap()
            );
        }
        thread::sleep(Duration::from_millis(10));
    };
    let text = std::fs::read_to_string(&stdout).unwrap();
    assert!(
        status.success(),
        "{sdk} driver: {status}\n{text}\n{}",
        std::fs::read_to_string(&stderr).unwrap()
    );
    assert!(
        text.contains(&format!("PASS linux-{sdk} retry completion stream")),
        "{text}"
    );
    server
        .finish()
        .unwrap_or_else(|e| panic!("{sdk} mock: {e}"));
}

#[test]
fn linux_openai_retry_completion_and_sse() {
    roundtrip(false);
}

#[test]
fn linux_anthropic_retry_completion_and_sse() {
    roundtrip(true);
}
