//! Dependency-free macOS/Linux HTTP transport bridge over system libcurl.
//!
//! macOS ships libcurl (SecureTransport-backed TLS) in libSystem's neighbourhood
//! (`/usr/lib/libcurl.dylib`); this file binds it through the v0.55.0 Rust
//! interop so `stdlib/net/http.ax` can move HTTPS bytes on macOS the way WinHTTP
//! does on Windows. Aoxn has no TLS stack of its own and `extern def` cannot
//! express libcurl's variadic `curl_easy_setopt` (the same fixed-arity trap that
//! bans `snprintf`), so the variadic calls live HERE, in real Rust, where the
//! compiler emits a correct variadic call site.
//!
//! Compile as an edition-2021 staticlib. The `#[link(name = "curl")]` attribute
//! propagates `-lcurl` through rustc's `native-static-libs` note, which the Aoxn
//! link step consumes — no manual `-l` flag is needed by the program.
//!
//! ABI (matches the guarded `extern def`s in stdlib/net/http.ax):
//!   * `string` parameters are `*const c_char` (UTF-8, NUL-terminated).
//!   * `int` parameters/returns are `i64`.
//!   * Returned strings/buffers are malloc'd, NUL-terminated and ADOPTED by the
//!     caller (Aoxn strings leak by design, exactly like the WinHTTP path's
//!     `as_string(buf)`); never free them through this bridge.
//!   * `out` slots are caller-owned, aligned `i64` arrays.
//!
//! Other platforms are inert: every export reports a clean transport error or
//! returns 0/-1, so the file still compiles and links into a Windows staticlib
//! (the Aoxn scanner is not a cfg evaluator — keep exports top-level).
//! No Apple SDK/link/runtime validation is implied by a cross-target metadata
//! check; native execution is verified by the macOS CI job.
#![allow(unused_variables)]
use std::os::raw::c_char;

#[cfg(any(target_os = "macos", target_os = "linux"))]
mod imp {
    use std::collections::BTreeMap;
    use std::ffi::CStr;
    use std::os::raw::{c_char, c_int, c_long, c_void};
    use std::ptr::null_mut;
    use std::sync::atomic::{AtomicI64, Ordering};
    use std::sync::{Arc, Condvar, Mutex, Once};
    use std::thread::JoinHandle;

    // ---- libcurl FFI ----
    #[link(name = "curl")]
    extern "C" {
        fn curl_easy_init() -> *mut c_void;
        fn curl_easy_cleanup(h: *mut c_void);
        fn curl_easy_setopt(h: *mut c_void, opt: c_int, ...) -> c_int;
        fn curl_easy_perform(h: *mut c_void) -> c_int;
        fn curl_easy_strerror(code: c_int) -> *const c_char;
        fn curl_slist_append(list: *mut c_void, s: *const c_char) -> *mut c_void;
        fn curl_slist_free_all(list: *mut c_void);
        fn curl_global_init(flags: c_long) -> c_int;
    }
    extern "C" {
        fn malloc(n: usize) -> *mut c_void;
    }

    // CURLoption / CURLINFO constants (decimal — the repo keeps constants decimal).
    const CURLOPT_URL: c_int = 10002;
    const CURLOPT_POSTFIELDS: c_int = 10015;
    const CURLOPT_USERAGENT: c_int = 10018;
    const CURLOPT_HTTPHEADER: c_int = 10023;
    const CURLOPT_HEADERDATA: c_int = 10029;
    const CURLOPT_CUSTOMREQUEST: c_int = 10036;
    const CURLOPT_WRITEFUNCTION: c_int = 20011;
    const CURLOPT_WRITEDATA: c_int = 10001;
    const CURLOPT_HEADERFUNCTION: c_int = 20079;
    const CURLOPT_FOLLOWLOCATION: c_int = 52;
    const CURLOPT_NOBODY: c_int = 42;
    const CURLOPT_NOSIGNAL: c_int = 99;
    const CURLOPT_TIMEOUT_MS: c_int = 155;
    const CURLOPT_CONNECTTIMEOUT_MS: c_int = 156;
    const CURLOPT_POSTFIELDSIZE: c_int = 60;
    const CURLOPT_PROTOCOLS: c_int = 181;
    const CURLOPT_SUPPRESS_CONNECT_HEADERS: c_int = 265;
    const CURL_GLOBAL_ALL: c_long = 3;

    static INIT: Once = Once::new();
    fn global_init() {
        INIT.call_once(|| unsafe {
            curl_global_init(CURL_GLOBAL_ALL);
        });
    }

    // malloc a NUL-terminated copy the Aoxn caller adopts as a string/buffer.
    unsafe fn leak_bytes(data: &[u8]) -> *mut c_char {
        let p = malloc(data.len() + 1) as *mut u8;
        if p.is_null() {
            return null_mut();
        }
        std::ptr::copy_nonoverlapping(data.as_ptr(), p, data.len());
        *p.add(data.len()) = 0;
        p as *mut c_char
    }

    unsafe fn cstr<'a>(p: *const c_char) -> &'a str {
        if p.is_null() {
            return "";
        }
        CStr::from_ptr(p).to_str().unwrap_or("")
    }

    // Build a curl header list from the CRLF-separated "Name: value" block the
    // SDKs assemble. Empty lines (the block's trailing CRLF) are skipped.
    unsafe fn header_list(headers: *const c_char) -> *mut c_void {
        let mut list: *mut c_void = null_mut();
        let s = cstr(headers);
        for line in s.split("\r\n") {
            let line = line.trim_end_matches('\n');
            if line.is_empty() {
                continue;
            }
            let Ok(c) = std::ffi::CString::new(line) else {
                continue;
            };
            list = curl_slist_append(list, c.as_ptr());
        }
        list
    }

    // Collecting write/header callback: append the delivered bytes to a Vec.
    unsafe extern "C" fn collect_cb(
        ptr: *mut c_char,
        size: usize,
        nmemb: usize,
        userdata: *mut c_void,
    ) -> usize {
        let n = size * nmemb;
        let v = &mut *(userdata as *mut Vec<u8>);
        v.extend_from_slice(std::slice::from_raw_parts(ptr as *const u8, n));
        n
    }

    // Configure the easy handle shared by the request and streaming paths.
    // Returns the header list (caller frees) or null on a setup failure.
    unsafe fn configure(
        h: *mut c_void,
        method: *const c_char,
        url: *const c_char,
        headers: *const c_char,
        body_ptr: i64,
        body_len: i64,
        ua: *const c_char,
        connect_ms: i64,
        recv_ms: i64,
    ) -> *mut c_void {
        let url_c = std::ffi::CString::new(cstr(url)).unwrap_or_default();
        curl_easy_setopt(h, CURLOPT_URL, url_c.as_ptr());
        let method_s = cstr(method);
        let method_c = std::ffi::CString::new(method_s).unwrap_or_default();
        curl_easy_setopt(h, CURLOPT_CUSTOMREQUEST, method_c.as_ptr());
        if method_s == "HEAD" {
            curl_easy_setopt(h, CURLOPT_NOBODY, 1 as c_long);
        }
        let ua_c = std::ffi::CString::new(cstr(ua)).unwrap_or_default();
        curl_easy_setopt(h, CURLOPT_USERAGENT, ua_c.as_ptr());
        curl_easy_setopt(h, CURLOPT_CONNECTTIMEOUT_MS, connect_ms as c_long);
        curl_easy_setopt(h, CURLOPT_TIMEOUT_MS, recv_ms as c_long);
        curl_easy_setopt(h, CURLOPT_NOSIGNAL, 1 as c_long);
        // Never follow redirects: a 3xx is delivered as itself so the code
        // holding the credential decides what to re-send (the WinHTTP path's
        // WINHTTP_OPTION_REDIRECT_POLICY_NEVER, mirrored here).
        curl_easy_setopt(h, CURLOPT_FOLLOWLOCATION, 0 as c_long);
        // Only HTTP(S), with libcurl's default peer/hostname verification intact.
        curl_easy_setopt(h, CURLOPT_PROTOCOLS, 3 as c_long);
        // Proxy CONNECT headers are not the origin's final response headers.
        curl_easy_setopt(h, CURLOPT_SUPPRESS_CONNECT_HEADERS, 1 as c_long);
        if body_len > 0 && body_ptr != 0 {
            curl_easy_setopt(h, CURLOPT_POSTFIELDS, body_ptr as *const c_void);
            curl_easy_setopt(h, CURLOPT_POSTFIELDSIZE, body_len as c_long);
        }
        header_list(headers)
    }

    // ---- one-shot request ----
    pub unsafe fn request(
        method: *const c_char,
        url: *const c_char,
        headers: *const c_char,
        body_ptr: i64,
        body_len: i64,
        ua: *const c_char,
        connect_ms: i64,
        recv_ms: i64,
        out: *mut i64,
    ) -> i64 {
        global_init();
        for i in 0..5 {
            *out.add(i) = 0;
        }
        let h = curl_easy_init();
        if h.is_null() {
            *out.add(4) = leak_bytes(b"curl_easy_init failed") as i64;
            return 1;
        }
        let list = configure(h, method, url, headers, body_ptr, body_len, ua, connect_ms, recv_ms);
        curl_easy_setopt(h, CURLOPT_HTTPHEADER, list);
        let mut body: Vec<u8> = Vec::new();
        let mut hdrs: Vec<u8> = Vec::new();
        curl_easy_setopt(h, CURLOPT_WRITEFUNCTION, collect_cb as *const c_void);
        curl_easy_setopt(h, CURLOPT_WRITEDATA, &mut body as *mut Vec<u8>);
        curl_easy_setopt(h, CURLOPT_HEADERFUNCTION, collect_cb as *const c_void);
        curl_easy_setopt(h, CURLOPT_HEADERDATA, &mut hdrs as *mut Vec<u8>);
        let rc = curl_easy_perform(h);
        if rc != 0 {
            let msg = CStr::from_ptr(curl_easy_strerror(rc)).to_string_lossy().into_owned();
            *out.add(4) = leak_bytes(msg.as_bytes()) as i64;
            curl_slist_free_all(list);
            curl_easy_cleanup(h);
            return 1;
        }
        // The status line is the first header line ("HTTP/1.1 200 OK"); parse
        // the code from it so we need no cross-thread getinfo on the easy handle.
        let status = parse_status(&hdrs);
        *out.add(0) = status;
        *out.add(1) = leak_bytes(&hdrs) as i64;
        *out.add(2) = leak_bytes(&body) as i64;
        *out.add(3) = body.len() as i64;
        *out.add(4) = leak_bytes(b"") as i64;
        curl_slist_free_all(list);
        curl_easy_cleanup(h);
    0
    }

    // Parse the last HTTP status code out of a raw header block.
    fn parse_status(hdrs: &[u8]) -> i64 {
        let mut status = 0;
        for line in hdrs.split(|b| *b == b'\n') {
            if line.starts_with(b"HTTP/") {
                // HTTP/2 and HTTP/3 have shorter version tokens than HTTP/1.1.
                let code = line.split(|b| b.is_ascii_whitespace()).nth(1).unwrap_or_default();
                if code.len() == 3 && code.iter().all(|c| c.is_ascii_digit()) {
                    status = (code[0] - b'0') as i64 * 100
                        + (code[1] - b'0') as i64 * 10
                        + (code[2] - b'0') as i64;
                }
            }
        }
        status
    }

    // ---- streaming ----
    struct StreamBuf {
        data: Vec<u8>,
        pos: usize,
        headers: Vec<u8>,
        headers_ready: bool,
        done: bool,
        cancelled: bool,
        status: i64,
        err: String,
    }
    struct Stream {
        buf: Arc<(Mutex<StreamBuf>, Condvar)>,
        thread: Option<JoinHandle<()>>,
    }
    static STREAMS: Mutex<BTreeMap<i64, Stream>> = Mutex::new(BTreeMap::new());
    static NEXT_ID: AtomicI64 = AtomicI64::new(1);

    unsafe extern "C" fn stream_write_cb(
        ptr: *mut c_char,
        size: usize,
        nmemb: usize,
        userdata: *mut c_void,
    ) -> usize {
        let n = size * nmemb;
        let pair = &*(userdata as *const (Mutex<StreamBuf>, Condvar));
        let mut g = pair.0.lock().unwrap();
        if g.cancelled {
            return 0; // abort the transfer (CURLE_WRITE_ERROR)
        }
        g.data.extend_from_slice(std::slice::from_raw_parts(ptr as *const u8, n));
        pair.1.notify_all();
        n
    }

    unsafe extern "C" fn stream_header_cb(
        ptr: *mut c_char,
        size: usize,
        nmemb: usize,
        userdata: *mut c_void,
    ) -> usize {
        let n = size * nmemb;
        let bytes = std::slice::from_raw_parts(ptr as *const u8, n);
        let pair = &*(userdata as *const (Mutex<StreamBuf>, Condvar));
        let mut g = pair.0.lock().unwrap();
        if bytes.starts_with(b"HTTP/") {
            g.headers.clear();
            g.status = parse_status(bytes);
        }
        g.headers.extend_from_slice(bytes);
        // Ignore interim 1xx blocks; CONNECT blocks are suppressed by configure.
        if (bytes == b"\r\n" || bytes == b"\n") && g.status >= 200 {
            g.headers_ready = true;
            pair.1.notify_all();
        }
        n
    }

    pub unsafe fn stream_open(
        method: *const c_char,
        url: *const c_char,
        headers: *const c_char,
        body_ptr: i64,
        body_len: i64,
        ua: *const c_char,
        connect_ms: i64,
        recv_ms: i64,
        out: *mut i64,
    ) -> i64 {
        global_init();
        for i in 0..3 {
            *out.add(i) = 0;
        }
        let pair = Arc::new((
            Mutex::new(StreamBuf {
                data: Vec::new(),
                pos: 0,
                headers: Vec::new(),
                headers_ready: false,
                done: false,
                cancelled: false,
                status: 0,
                err: String::new(),
            }),
            Condvar::new(),
        ));
        // The perform thread owns its own easy handle and header list.
        let thread_pair = pair.clone();
        let method_s = cstr(method).to_string();
        let url_s = cstr(url).to_string();
        let headers_s = cstr(headers).to_string();
        let ua_s = cstr(ua).to_string();
        // The caller may free its upload as soon as open returns. The worker
        // must own the bytes even if the server sends headers before upload ends.
        let body = if body_ptr != 0 && body_len > 0 {
            std::slice::from_raw_parts(body_ptr as *const u8, body_len as usize).to_vec()
        } else {
            Vec::new()
        };
        let thread = std::thread::spawn(move || unsafe {
            let h = curl_easy_init();
            if h.is_null() {
                let mut g = thread_pair.0.lock().unwrap();
                g.err = "curl_easy_init failed".to_string();
                g.done = true;
                thread_pair.1.notify_all();
                return;
            }
            let m = std::ffi::CString::new(method_s).unwrap_or_default();
            let u = std::ffi::CString::new(url_s).unwrap_or_default();
            let hd = std::ffi::CString::new(headers_s).unwrap_or_default();
            let a = std::ffi::CString::new(ua_s).unwrap_or_default();
            let list = configure(h, m.as_ptr(), u.as_ptr(), hd.as_ptr(), body.as_ptr() as i64, body.len() as i64, a.as_ptr(), connect_ms, recv_ms);
            curl_easy_setopt(h, CURLOPT_HTTPHEADER, list);
            let ud = &*thread_pair as *const (Mutex<StreamBuf>, Condvar) as *mut c_void;
            curl_easy_setopt(h, CURLOPT_WRITEFUNCTION, stream_write_cb as *const c_void);
            curl_easy_setopt(h, CURLOPT_WRITEDATA, ud);
            curl_easy_setopt(h, CURLOPT_HEADERFUNCTION, stream_header_cb as *const c_void);
            curl_easy_setopt(h, CURLOPT_HEADERDATA, ud);
            let rc = curl_easy_perform(h);
            {
                let mut g = thread_pair.0.lock().unwrap();
                if rc != 0 && !g.cancelled {
                    g.err = CStr::from_ptr(curl_easy_strerror(rc)).to_string_lossy().into_owned();
                }
                g.done = true;
                thread_pair.1.notify_all();
            }
            curl_slist_free_all(list);
            curl_easy_cleanup(h);
        });

        // Wait until the header block arrives (or the transfer fails first).
        let (lock, cv) = &*pair;
        let mut g = lock.lock().unwrap();
        while !g.headers_ready && !g.done {
            g = cv.wait(g).unwrap();
        }
        if !g.headers_ready {
            let msg = if g.err.is_empty() { "no response headers".to_string() } else { g.err.clone() };
            *out.add(2) = leak_bytes(msg.as_bytes()) as i64;
            g.cancelled = true;
            drop(g);
            // Detach: the thread finishes on its own and releases the handle.
            return 0;
        }
        let status = g.status;
        let hdrs = g.headers.clone();
        drop(g);
        let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
        STREAMS.lock().unwrap().insert(id, Stream { buf: pair, thread: Some(thread) });
        *out.add(0) = status;
        *out.add(1) = leak_bytes(&hdrs) as i64;
        *out.add(2) = leak_bytes(b"") as i64;
        id
    }

    pub unsafe fn stream_read(handle: i64, buf: i64, cap: i64) -> i64 {
        let pair = {
            let streams = STREAMS.lock().unwrap();
            match streams.get(&handle) {
                Some(s) => s.buf.clone(),
                None => return -1,
            }
        };
        let (lock, cv) = &*pair;
        let mut g = lock.lock().unwrap();
        loop {
            let avail = g.data.len() - g.pos;
            if avail > 0 {
                let n = avail.min(cap.max(0) as usize);
                std::ptr::copy_nonoverlapping(g.data.as_ptr().add(g.pos), buf as *mut u8, n);
                g.pos += n;
                // Reclaim drained bytes so a long SSE stream does not grow forever.
                if g.pos > 65536 && g.pos == g.data.len() {
                    g.data.clear();
                    g.pos = 0;
                }
                return n as i64;
            }
            if g.done {
                return if g.err.is_empty() { 0 } else { -1 };
            }
            if g.cancelled {
                return -1;
            }
            g = cv.wait(g).unwrap();
        }
    }

    pub unsafe fn stream_close(handle: i64) {
        let removed = STREAMS.lock().unwrap().remove(&handle);
        if let Some(mut s) = removed {
            {
                let (lock, cv) = &*s.buf;
                let mut g = lock.lock().unwrap();
                g.cancelled = true;
                cv.notify_all();
            }
            // Detach rather than join: an idle SSE connection would otherwise
            // block close until the receive timeout. The thread aborts on its
            // next write callback (or timeout) and releases the easy handle.
            let _ = s.thread.take();
        }
    }

    pub unsafe fn stream_err(handle: i64) -> *const c_char {
        let streams = STREAMS.lock().unwrap();
        match streams.get(&handle) {
            Some(s) => {
                let g = s.buf.0.lock().unwrap();
                leak_bytes(g.err.as_bytes())
            }
            None => leak_bytes(b""),
        }
    }
}

// Keep exports unconditional and top-level: the Aoxn scanner is not a cfg evaluator.

#[no_mangle]
pub extern "C" fn aoxn_http_request(
    method: *const c_char,
    url: *const c_char,
    headers: *const c_char,
    body_ptr: i64,
    body_len: i64,
    ua: *const c_char,
    connect_ms: i64,
    recv_ms: i64,
    out: i64,
) -> i64 {
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    unsafe {
        return imp::request(method, url, headers, body_ptr, body_len, ua, connect_ms, recv_ms, out as *mut i64);
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        unsafe {
            let out = out as *mut i64;
            for i in 0..5 {
                *out.add(i) = 0;
            }
            *out.add(4) = b"net/http: libcurl transport requires macOS or Linux\0".as_ptr() as i64;
        }
        1
    }
}

#[no_mangle]
pub extern "C" fn aoxn_http_stream_open(
    method: *const c_char,
    url: *const c_char,
    headers: *const c_char,
    body_ptr: i64,
    body_len: i64,
    ua: *const c_char,
    connect_ms: i64,
    recv_ms: i64,
    out: i64,
) -> i64 {
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    unsafe {
        return imp::stream_open(method, url, headers, body_ptr, body_len, ua, connect_ms, recv_ms, out as *mut i64);
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        unsafe {
            let out = out as *mut i64;
            for i in 0..3 {
                *out.add(i) = 0;
            }
            *out.add(2) = b"net/http: libcurl transport requires macOS or Linux\0".as_ptr() as i64;
        }
        0
    }
}

#[no_mangle]
pub extern "C" fn aoxn_http_stream_read(handle: i64, buf: i64, cap: i64) -> i64 {
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    unsafe {
        return imp::stream_read(handle, buf, cap);
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        -1
    }
}

#[no_mangle]
pub extern "C" fn aoxn_http_stream_close(handle: i64) {
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    unsafe {
        imp::stream_close(handle);
    }
}

#[no_mangle]
pub extern "C" fn aoxn_http_stream_err(handle: i64) -> *const c_char {
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    unsafe {
        return imp::stream_err(handle);
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        b"\0".as_ptr() as *const c_char
    }
}