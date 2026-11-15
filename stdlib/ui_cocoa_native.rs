//! Dependency-free Cocoa bridge. Compile as an edition-2021 staticlib.
//! Coordinates/sizes are logical points, origin top-left; colors are COLORREF.
//! All calls must run on the OS main thread. Invalid/closed handles are ignored;
//! open/alive/measure/size return zero on failure. Handles are monotonic IDs,
//! not addresses. close hides/closes the window; free also releases its backing.
//! pump requires a writable, aligned i64[1024]: 0/1 mouse, 2/3 held buttons,
//! 4 wheel (120 per line), 8..263 held VK keys, 264 UTF-16 count, 265..1023
//! UTF-16 units. Remaining input is retained for later pumps, never overrun.
//! The raw pointer/string validity contract belongs to the caller. Text input
//! samples NSEvent characters (including repeats); this is not an IME client.
//! Link on macOS with AppKit, CoreGraphics, Foundation and objc (link attributes
//! below propagate these requirements). No Apple SDK/link/runtime validation is
//! implied by a cross-target metadata check. Non-Apple builds are inert.
#![allow(unused_variables)]
use std::os::raw::c_char;

#[cfg(target_os = "macos")]
mod mac {
    use super::c_char;
    use std::cell::RefCell;
    use std::collections::{BTreeMap, VecDeque};
    use std::ffi::{c_void, CStr, CString};
    use std::mem::transmute;
    use std::ptr::{null, null_mut};

    type Id = *mut c_void;
    type Sel = *mut c_void;
    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    struct Point {
        x: f64,
        y: f64,
    }
    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    struct Size {
        w: f64,
        h: f64,
    }
    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    struct Rect {
        origin: Point,
        size: Size,
    }
    fn rect(x: f64, y: f64, w: f64, h: f64) -> Rect {
        Rect {
            origin: Point { x, y },
            size: Size { w, h },
        }
    }

    #[link(name = "objc")]
    extern "C" {
        fn objc_getClass(name: *const c_char) -> Id;
        fn sel_registerName(name: *const c_char) -> Sel;
        fn objc_msgSend();
        #[cfg(target_arch = "x86_64")]
        fn objc_msgSend_stret();
        fn objc_allocateClassPair(superclass: Id, name: *const c_char, extra: usize) -> Id;
        fn objc_registerClassPair(class: Id);
        fn class_addMethod(
            class: Id,
            selector: Sel,
            imp: *const c_void,
            types: *const c_char,
        ) -> i8;
    }
    #[link(name = "Foundation", kind = "framework")]
    extern "C" {
        fn pthread_main_np() -> i32;
        static NSDefaultRunLoopMode: Id;
    }
    #[link(name = "AppKit", kind = "framework")]
    extern "C" {
        static NSFontAttributeName: Id;
        static NSForegroundColorAttributeName: Id;
    }
    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGColorSpaceCreateDeviceRGB() -> Id;
        fn CGColorSpaceRelease(space: Id);
        fn CGBitmapContextCreate(
            data: *mut c_void,
            w: usize,
            h: usize,
            bits: usize,
            stride: usize,
            space: Id,
            info: u32,
        ) -> Id;
        fn CGBitmapContextCreateImage(ctx: Id) -> Id;
        fn CGContextRelease(ctx: Id);
        fn CGImageRelease(image: Id);
        fn CGContextSaveGState(ctx: Id);
        fn CGContextRestoreGState(ctx: Id);
        fn CGContextTranslateCTM(ctx: Id, x: f64, y: f64);
        fn CGContextScaleCTM(ctx: Id, x: f64, y: f64);
        fn CGContextSetRGBFillColor(ctx: Id, r: f64, g: f64, b: f64, a: f64);
        fn CGContextSetRGBStrokeColor(ctx: Id, r: f64, g: f64, b: f64, a: f64);
        fn CGContextSetLineWidth(ctx: Id, w: f64);
        fn CGContextFillRect(ctx: Id, r: Rect);
        fn CGContextStrokeRect(ctx: Id, r: Rect);
        fn CGContextStrokeEllipseInRect(ctx: Id, r: Rect);
        fn CGContextFillEllipseInRect(ctx: Id, r: Rect);
        fn CGContextBeginPath(ctx: Id);
        fn CGContextMoveToPoint(ctx: Id, x: f64, y: f64);
        fn CGContextAddLineToPoint(ctx: Id, x: f64, y: f64);
        fn CGContextStrokePath(ctx: Id);
        fn CGContextClipToRect(ctx: Id, r: Rect);
        fn CGContextDrawImage(ctx: Id, r: Rect, image: Id);
        fn CGPathCreateWithRoundedRect(r: Rect, rw: f64, rh: f64, transform: *const c_void) -> Id;
        fn CGContextAddPath(ctx: Id, path: Id);
        fn CGContextFillPath(ctx: Id);
        fn CGPathRelease(path: Id);
        fn CGEventSourceKeyState(source: i32, key: u16) -> bool;
    }

    // Every send is cast to its EXACT fixed-arity Objective-C ABI, never called
    // as a variadic function (especially important for ARM floating arguments).
    macro_rules! msg {
        ($obj:expr, $name:expr => $ret:ty $(, $ty:ty => $arg:expr)*) => {{
            let f: unsafe extern "C" fn(Id, Sel $(, $ty)*) -> $ret =
                transmute(objc_msgSend as *const ());
            f($obj, sel_registerName($name.as_ptr().cast()) $(, $arg)*)
        }};
    }
    unsafe fn class(name: &[u8]) -> Id {
        objc_getClass(name.as_ptr().cast())
    }
    unsafe fn release(o: Id) {
        if !o.is_null() {
            msg!(o, b"release\0" => ());
        }
    }
    struct Pool(Id);
    impl Pool {
        unsafe fn new() -> Self {
            Self(msg!(msg!(class(b"NSAutoreleasePool\0"), b"alloc\0" => Id), b"init\0" => Id))
        }
    }
    impl Drop for Pool {
        fn drop(&mut self) {
            unsafe {
                msg!(self.0, b"drain\0" => ());
            }
        }
    }
    pub fn main_thread() -> bool {
        unsafe { pthread_main_np() != 0 }
    }
    unsafe fn string(p: *const c_char) -> Id {
        let p = if p.is_null() {
            b"\0".as_ptr().cast()
        } else {
            p
        };
        let s = msg!(class(b"NSString\0"), b"stringWithUTF8String:\0" => Id, *const c_char => p);
        if s.is_null() {
            msg!(class(b"NSString\0"), b"stringWithUTF8String:\0" => Id, *const c_char => b"\0".as_ptr().cast())
        } else {
            s
        }
    }
    unsafe fn bounds(view: Id) -> Rect {
        // CGRect is 32 bytes: Intel requires stret; ARM64 uses normal msgSend.
        #[cfg(target_arch = "x86_64")]
        {
            let mut r = Rect::default();
            let f: unsafe extern "C" fn(*mut Rect, Id, Sel) =
                transmute(objc_msgSend_stret as *const ());
            f(&mut r, view, sel_registerName(b"bounds\0".as_ptr().cast()));
            r
        }
        #[cfg(not(target_arch = "x86_64"))]
        {
            msg!(view, b"bounds\0" => Rect)
        }
    }
    struct Surface {
        window: Id,
        view: Id,
        ctx: Id,
        w: i64,
        h: i64,
        clips: Vec<Rect>,
        text: VecDeque<u16>,
        wheel: f64,
        keys_pressed: [bool; 256],
        closed: bool,
    }
    impl Drop for Surface {
        fn drop(&mut self) {
            unsafe {
                msg!(self.window, b"close\0" => ());
                release(self.window);
                CGContextRelease(self.ctx);
            }
        }
    }
    struct Registry {
        next: i64,
        windows: BTreeMap<i64, Box<Surface>>,
        clipboard: CString,
    }
    impl Registry {
        fn new() -> Self {
            Self {
                next: 1,
                windows: BTreeMap::new(),
                clipboard: CString::default(),
            }
        }
    }
    thread_local! { static REG: RefCell<Registry> = RefCell::new(Registry::new()); }
    // Box allocations stay stable. No registry borrow is held over an AppKit
    // call, since display/sendEvent can synchronously call back into drawRect.
    fn surface(h: i64) -> Option<*mut Surface> {
        if !main_thread() {
            return None;
        }
        REG.with(|r| {
            r.borrow_mut()
                .windows
                .get_mut(&h)
                .map(|s| &mut **s as *mut Surface)
        })
    }
    unsafe fn by_window(window: Id) -> Option<*mut Surface> {
        REG.with(|r| {
            r.borrow_mut()
                .windows
                .values_mut()
                .find(|s| s.window == window)
                .map(|s| &mut **s as *mut Surface)
        })
    }
    unsafe extern "C" fn draw(view: Id, _: Sel, _: Rect) {
        let ctx = REG.with(|r| {
            r.borrow()
                .windows
                .values()
                .find(|s| s.view == view)
                .map(|s| s.ctx)
        });
        if let Some(ctx) = ctx {
            let image = CGBitmapContextCreateImage(ctx);
            if !image.is_null() {
                let gc = msg!(class(b"NSGraphicsContext\0"), b"currentContext\0" => Id);
                let dest = msg!(gc, b"CGContext\0" => Id);
                if !dest.is_null() {
                    CGContextDrawImage(dest, bounds(view), image);
                }
                CGImageRelease(image);
            }
        }
    }
    unsafe extern "C" fn yes(_: Id, _: Sel) -> i8 {
        1
    }
    unsafe extern "C" fn key_down(_: Id, _: Sel, _: Id) { /* sampled before sendEvent */
    }
    unsafe extern "C" fn will_close(_: Id, _: Sel, notification: Id) {
        let window = msg!(notification, b"object\0" => Id);
        if let Some(s) = by_window(window) {
            (*s).closed = true;
        }
    }
    unsafe fn view_class() -> Id {
        let name = b"AoxnCocoaNativeCanvas_v1\0";
        let existing = class(name);
        if !existing.is_null() {
            return existing;
        }
        let c = objc_allocateClassPair(class(b"NSView\0"), name.as_ptr().cast(), 0);
        if c.is_null() {
            return null_mut();
        }
        class_addMethod(
            c,
            sel_registerName(b"drawRect:\0".as_ptr().cast()),
            draw as *const c_void,
            b"v@:{CGRect={CGPoint=dd}{CGSize=dd}}\0".as_ptr().cast(),
        );
        class_addMethod(
            c,
            sel_registerName(b"acceptsFirstResponder\0".as_ptr().cast()),
            yes as *const c_void,
            b"c@:\0".as_ptr().cast(),
        );
        class_addMethod(
            c,
            sel_registerName(b"keyDown:\0".as_ptr().cast()),
            key_down as *const c_void,
            b"v@:@\0".as_ptr().cast(),
        );
        class_addMethod(
            c,
            sel_registerName(b"windowWillClose:\0".as_ptr().cast()),
            will_close as *const c_void,
            b"v@:@\0".as_ptr().cast(),
        );
        objc_registerClassPair(c);
        c
    }
    unsafe fn bitmap(w: i64, h: i64) -> Id {
        // Bound both dimensions and total storage (max 256 MiB); Cocoa owns data.
        if w < 1 || h < 1 || w > 16384 || h > 16384 || w * h > 67_108_864 {
            return null_mut();
        }
        let space = CGColorSpaceCreateDeviceRGB();
        if space.is_null() {
            return null_mut();
        }
        let c = CGBitmapContextCreate(
            null_mut(),
            w as usize,
            h as usize,
            8,
            w as usize * 4,
            space,
            1,
        );
        CGColorSpaceRelease(space); // premultiplied-last, native default byte order
        if !c.is_null() {
            CGContextSetRGBFillColor(c, 1.0, 1.0, 1.0, 1.0);
            CGContextFillRect(c, rect(0.0, 0.0, w as f64, h as f64));
            CGContextTranslateCTM(c, 0.0, h as f64);
            CGContextScaleCTM(c, 1.0, -1.0);
        }
        c
    }
    unsafe fn sync_size(s: *mut Surface) {
        let size = bounds((*s).view).size;
        let w = size.w.round() as i64;
        let h = size.h.round() as i64;
        if w == (*s).w && h == (*s).h {
            return;
        }
        let c = bitmap(w, h);
        if c.is_null() {
            return;
        } // retain the last valid backing on allocation failure
        let old = (*s).ctx;
        (*s).ctx = c;
        (*s).w = w;
        (*s).h = h;
        // Clip stack remains logical across resize; replay it on the new context.
        for r in &(*s).clips {
            CGContextSaveGState(c);
            CGContextClipToRect(c, *r);
        }
        CGContextRelease(old);
    }
    pub unsafe fn open(title: *const c_char, w: i64, h: i64) -> i64 {
        if !main_thread() {
            return 0;
        }
        let _pool = Pool::new();
        let ctx = bitmap(w, h);
        if ctx.is_null() {
            return 0;
        }
        let app = msg!(class(b"NSApplication\0"), b"sharedApplication\0" => Id);
        msg!(app, b"setActivationPolicy:\0" => i8, i64 => 0);
        msg!(app, b"finishLaunching\0" => ());
        let window = msg!(msg!(class(b"NSWindow\0"), b"alloc\0" => Id), b"initWithContentRect:styleMask:backing:defer:\0" => Id,
            Rect => rect(0.0, 0.0, w as f64, h as f64), u64 => 15, u64 => 2, i8 => 0);
        if window.is_null() {
            CGContextRelease(ctx);
            return 0;
        }
        msg!(window, b"setReleasedWhenClosed:\0" => (), i8 => 0);
        let vc = view_class();
        let view = if vc.is_null() {
            null_mut()
        } else {
            msg!(msg!(vc, b"alloc\0" => Id), b"initWithFrame:\0" => Id, Rect => rect(0.0, 0.0, w as f64, h as f64))
        };
        if view.is_null() {
            release(window);
            CGContextRelease(ctx);
            return 0;
        }
        msg!(window, b"setContentView:\0" => (), Id => view);
        msg!(window, b"setDelegate:\0" => (), Id => view);
        release(view); // window retains the view
        msg!(window, b"setTitle:\0" => (), Id => string(title));
        msg!(window, b"setAcceptsMouseMovedEvents:\0" => (), i8 => 1);
        msg!(window, b"makeFirstResponder:\0" => i8, Id => view);
        msg!(window, b"center\0" => ());
        let id = REG.with(|r| {
            let mut r = r.borrow_mut();
            let id = r.next;
            if id == i64::MAX {
                return 0;
            }
            r.next += 1;
            r.windows.insert(
                id,
                Box::new(Surface {
                    window,
                    view,
                    ctx,
                    w,
                    h,
                    clips: Vec::new(),
                    text: VecDeque::new(),
                    wheel: 0.0,
                    keys_pressed: [false; 256],
                    closed: false,
                }),
            );
            id
        });
        if id == 0 {
            msg!(window, b"close\0" => ());
            release(window);
            CGContextRelease(ctx);
            return 0;
        }
        msg!(window, b"makeKeyAndOrderFront:\0" => (), Id => null_mut());
        msg!(app, b"activateIgnoringOtherApps:\0" => (), i8 => 1);
        id
    }
    pub unsafe fn alive(h: i64) -> i64 {
        surface(h).map_or(0, |s| (!(*s).closed) as i64)
    }
    pub unsafe fn dimension(h: i64, height: bool) -> i64 {
        surface(h).map_or(0, |s| {
            sync_size(s);
            if height {
                (*s).h
            } else {
                (*s).w
            }
        })
    }
    pub unsafe fn resize(h: i64, w: i64, height: i64) {
        if w < 1 || height < 1 || w > 16384 || height > 16384 || w * height > 67_108_864 {
            return;
        }
        if let Some(s) = surface(h) {
            if (*s).closed {
                return;
            }
            let _pool = Pool::new();
            msg!((*s).window, b"setContentSize:\0" => (), Size => Size { w: w as f64, h: height as f64 });
            sync_size(s);
        }
    }
    pub unsafe fn present(h: i64) {
        if let Some(s) = surface(h) {
            if (*s).closed {
                return;
            }
            let _pool = Pool::new();
            sync_size(s);
            msg!((*s).view, b"setNeedsDisplay:\0" => (), i8 => 1);
            msg!((*s).view, b"displayIfNeeded\0" => ());
        }
    }
    fn rgb(color: i64) -> (f64, f64, f64) {
        (
            (color & 255) as f64 / 255.0,
            ((color >> 8) & 255) as f64 / 255.0,
            ((color >> 16) & 255) as f64 / 255.0,
        )
    }
    pub unsafe fn shape(
        h: i64,
        kind: i64,
        x: i64,
        y: i64,
        w: i64,
        height: i64,
        fill: i64,
        border: i64,
    ) {
        if let Some(s) = surface(h) {
            if (*s).closed {
                return;
            }
            let c = (*s).ctx;
            let (r, g, b) = rgb(fill);
            CGContextSetRGBFillColor(c, r, g, b, 1.0);
            let (r, g, b) = rgb(border);
            CGContextSetRGBStrokeColor(c, r, g, b, 1.0);
            CGContextSetLineWidth(c, 1.0);
            let rr = rect(x as f64, y as f64, w.max(0) as f64, height.max(0) as f64);
            match kind {
                0 => CGContextFillRect(c, rr),
                1 => CGContextStrokeRect(
                    c,
                    rect(
                        x as f64 + 0.5,
                        y as f64 + 0.5,
                        w.saturating_sub(1).max(0) as f64,
                        height.saturating_sub(1).max(0) as f64,
                    ),
                ),
                2 => {
                    let radius = 6.0_f64.min(rr.size.w / 2.0).min(rr.size.h / 2.0);
                    let path = CGPathCreateWithRoundedRect(rr, radius, radius, null());
                    if !path.is_null() {
                        CGContextBeginPath(c);
                        CGContextAddPath(c, path);
                        CGContextFillPath(c);
                        CGContextBeginPath(c);
                        CGContextAddPath(c, path);
                        CGContextStrokePath(c);
                        CGPathRelease(path);
                    }
                }
                3 => {
                    CGContextFillEllipseInRect(c, rr);
                    CGContextStrokeEllipseInRect(c, rr);
                }
                4 => CGContextFillEllipseInRect(c, rr),
                5 => {
                    CGContextBeginPath(c);
                    CGContextMoveToPoint(c, x as f64 + 0.5, y as f64 + 0.5);
                    CGContextAddLineToPoint(c, w as f64 + 0.5, height as f64 + 0.5);
                    CGContextStrokePath(c);
                }
                _ => {}
            }
        }
    }
    unsafe fn attributes(font: i64, color: i64) -> Id {
        let f = if font == 2 {
            msg!(class(b"NSFont\0"), b"userFixedPitchFontOfSize:\0" => Id, f64 => 13.0)
        } else {
            msg!(class(b"NSFont\0"), b"systemFontOfSize:\0" => Id, f64 => if font == 1 { 20.0 } else { 13.0 })
        };
        let (r, g, b) = rgb(color);
        let c = msg!(class(b"NSColor\0"), b"colorWithSRGBRed:green:blue:alpha:\0" => Id, f64 => r, f64 => g, f64 => b, f64 => 1.0);
        let d = msg!(class(b"NSMutableDictionary\0"), b"dictionary\0" => Id);
        if !f.is_null() {
            msg!(d, b"setObject:forKey:\0" => (), Id => f, Id => NSFontAttributeName);
        }
        msg!(d, b"setObject:forKey:\0" => (), Id => c, Id => NSForegroundColorAttributeName);
        d
    }
    pub unsafe fn text(h: i64, x: i64, y: i64, text: *const c_char, color: i64, font: i64) {
        if let Some(s) = surface(h) {
            if (*s).closed {
                return;
            }
            let _pool = Pool::new();
            let gc_class = class(b"NSGraphicsContext\0");
            msg!(gc_class, b"saveGraphicsState\0" => ());
            CGContextSaveGState((*s).ctx);
            let gc = msg!(gc_class, b"graphicsContextWithCGContext:flipped:\0" => Id, Id => (*s).ctx, i8 => 1);
            msg!(gc_class, b"setCurrentContext:\0" => (), Id => gc);
            msg!(string(text), b"drawAtPoint:withAttributes:\0" => (), Point => Point { x: x as f64, y: y as f64 }, Id => attributes(font, color));
            msg!(gc_class, b"restoreGraphicsState\0" => ());
            CGContextRestoreGState((*s).ctx);
        }
    }
    pub unsafe fn measure(text: *const c_char, font: i64, dimension: i64) -> i64 {
        if !main_thread() {
            return 0;
        }
        let _pool = Pool::new();
        // CGSize is two doubles and returned normally on both supported ABIs.
        let size = msg!(string(text), b"sizeWithAttributes:\0" => Size, Id => attributes(font, 0));
        if dimension == 0 {
            size.w.ceil() as i64
        } else {
            size.h.ceil() as i64
        }
    }
    pub unsafe fn clip(h: i64, l: i64, t: i64, r: i64, b: i64) {
        if let Some(s) = surface(h) {
            let rr = rect(
                l as f64,
                t as f64,
                (r as f64 - l as f64).max(0.0),
                (b as f64 - t as f64).max(0.0),
            );
            (*s).clips.push(rr);
            CGContextSaveGState((*s).ctx);
            CGContextClipToRect((*s).ctx, rr);
        }
    }
    pub unsafe fn unclip(h: i64) {
        if let Some(s) = surface(h) {
            if (*s).clips.pop().is_some() {
                CGContextRestoreGState((*s).ctx);
            }
        }
    }
    pub unsafe fn close(h: i64) {
        if let Some(s) = surface(h) {
            (*s).closed = true;
            msg!((*s).window, b"close\0" => ());
        }
    }
    pub unsafe fn free(h: i64) {
        if !main_thread() {
            return;
        }
        let _pool = Pool::new();
        // Drop outside RefCell borrow: closing can cause a synchronous redraw.
        let s = REG.with(|r| r.borrow_mut().windows.remove(&h));
        drop(s);
    }

    // Mac virtual hardware codes -> Windows VK protocol (US physical layout).
    // NSEvent characters, independently, respect the active keyboard layout.
    fn vk(k: u16) -> usize {
        match k {
            0 => 65,
            1 => 83,
            2 => 68,
            3 => 70,
            4 => 72,
            5 => 71,
            6 => 90,
            7 => 88,
            8 => 67,
            9 => 86,
            11 => 66,
            12 => 81,
            13 => 87,
            14 => 69,
            15 => 82,
            16 => 89,
            17 => 84,
            18 => 49,
            19 => 50,
            20 => 51,
            21 => 52,
            22 => 54,
            23 => 53,
            24 => 187,
            25 => 57,
            26 => 55,
            27 => 189,
            28 => 56,
            29 => 48,
            30 => 221,
            31 => 79,
            32 => 85,
            33 => 219,
            34 => 73,
            35 => 80,
            36 => 13,
            37 => 76,
            38 => 74,
            39 => 222,
            40 => 75,
            41 => 186,
            42 => 220,
            43 => 188,
            44 => 191,
            45 => 78,
            46 => 77,
            47 => 190,
            48 => 9,
            49 => 32,
            50 => 192,
            51 => 8,
            53 => 27,
            54 | 55 => 17,
            56 | 60 => 16,
            57 => 20,
            58 | 61 => 18,
            59 | 62 => 17,
            65 => 110,
            67 => 106,
            69 => 107,
            71 => 144,
            75 => 111,
            76 => 13,
            78 => 109,
            81 => 187,
            82 => 96,
            83 => 97,
            84 => 98,
            85 => 99,
            86 => 100,
            87 => 101,
            88 => 102,
            89 => 103,
            91 => 104,
            92 => 105,
            96 => 116,
            97 => 117,
            98 => 118,
            99 => 114,
            100 => 119,
            101 => 120,
            103 => 122,
            105 => 124,
            106 => 127,
            107 => 125,
            109 => 121,
            111 => 123,
            113 => 126,
            114 => 45,
            115 => 36,
            116 => 33,
            117 => 46,
            118 => 115,
            119 => 35,
            120 => 113,
            121 => 34,
            122 => 112,
            123 => 37,
            124 => 39,
            125 => 40,
            126 => 38,
            _ => 0,
        }
    }
    unsafe fn collect(event: Id) {
        let window = msg!(event, b"window\0" => Id);
        if let Some(s) = by_window(window) {
            let ty = msg!(event, b"type\0" => u64);
            if ty == 10 {
                let key = vk(msg!(event, b"keyCode\0" => u16));
                if key != 0 {
                    (*s).keys_pressed[key] = true;
                }
                let flags = msg!(event, b"modifierFlags\0" => u64);
                if flags & ((1 << 18) | (1 << 20)) == 0 {
                    // ctrl/cmd shortcuts are not text
                    let chars = msg!(event, b"characters\0" => Id);
                    let n = msg!(chars, b"length\0" => usize);
                    for i in 0..n.min(16384) {
                        let u = msg!(chars, b"characterAtIndex:\0" => u16, usize => i);
                        // Function-key private-use characters are not printable.
                        if (u >= 32 && !(0xf700..=0xf8ff).contains(&u)) || u == 9 || u == 13 {
                            if (*s).text.len() < 16384 {
                                (*s).text.push_back(u);
                            }
                        }
                    }
                }
            } else if ty == 22 {
                let d = msg!(event, b"scrollingDeltaY\0" => f64);
                let precise = msg!(event, b"hasPreciseScrollingDeltas\0" => i8) != 0;
                (*s).wheel += d * if precise { 12.0 } else { 120.0 };
            }
        }
    }
    pub unsafe fn pump(h: i64, state: i64, wait_ms: i64) {
        let Some(s) = surface(h) else {
            return;
        };
        if state == 0 || (state as usize) % std::mem::align_of::<i64>() != 0 {
            return;
        }
        let _pool = Pool::new();
        let app = msg!(class(b"NSApplication\0"), b"sharedApplication\0" => Id);
        // Wait only for the first event; drain thereafter without blocking.
        let mut date = msg!(class(b"NSDate\0"), b"dateWithTimeIntervalSinceNow:\0" => Id, f64 => wait_ms.clamp(0, 60000) as f64 / 1000.0);
        for _ in 0..4096 {
            let e = msg!(app, b"nextEventMatchingMask:untilDate:inMode:dequeue:\0" => Id,
                u64 => u64::MAX, Id => date, Id => NSDefaultRunLoopMode, i8 => 1);
            if e.is_null() {
                break;
            }
            collect(e); // all managed windows retain their own text/scroll input
            msg!(app, b"sendEvent:\0" => (), Id => e);
            date = msg!(class(b"NSDate\0"), b"distantPast\0" => Id);
        }
        msg!(app, b"updateWindows\0" => ());
        sync_size(s);
        let out = std::slice::from_raw_parts_mut(state as *mut i64, 1024);
        out.fill(0);
        let p = msg!((*s).window, b"mouseLocationOutsideOfEventStream\0" => Point);
        out[0] = p.x.floor() as i64;
        out[1] = ((*s).h as f64 - p.y).floor() as i64;
        let focused =
            msg!((*s).window, b"isKeyWindow\0" => i8) != 0 && msg!(app, b"isActive\0" => i8) != 0;
        if focused {
            for key in 0..256 {
                out[8 + key] |= (*s).keys_pressed[key] as i64;
            }
            let buttons = msg!(class(b"NSEvent\0"), b"pressedMouseButtons\0" => u64);
            out[2] = (buttons & 1 != 0) as i64;
            out[3] = (buttons & 2 != 0) as i64;
            for key in 0..128 {
                let v = vk(key);
                if v != 0 && CGEventSourceKeyState(1, key) {
                    out[8 + v] = 1;
                }
            }
            let flags = msg!(class(b"NSEvent\0"), b"modifierFlags\0" => u64);
            out[8 + 16] |= (flags & (1 << 17) != 0) as i64;
            out[8 + 17] |= (flags & ((1 << 18) | (1 << 20)) != 0) as i64;
            out[8 + 18] |= (flags & (1 << 19) != 0) as i64;
        }
        (*s).keys_pressed.fill(false);
        out[4] = (*s).wheel.trunc() as i64;
        (*s).wheel -= out[4] as f64;
        let mut n = (*s).text.len().min(256);
        // Do not split a surrogate pair at the output-buffer boundary.
        if n > 0 && (*s).text.len() > n {
            let last = (*s).text.get(n - 1).copied().unwrap_or(0);
            let next = (*s).text.get(n).copied().unwrap_or(0);
            if (0xd800..=0xdbff).contains(&last) && (0xdc00..=0xdfff).contains(&next) {
                n -= 1;
            }
        }
        for i in 0..n {
            out[265 + i] = (*s).text.pop_front().unwrap_or(0) as i64;
        }
        out[264] = n as i64;
        // A miniaturized window is not closed. NSWindowNumber becomes -1 on close.
        if msg!((*s).window, b"windowNumber\0" => i64) < 0 {
            (*s).closed = true;
        }
    }
    pub unsafe fn clipboard_set(text: *const c_char) {
        if !main_thread() {
            return;
        }
        let _pool = Pool::new();
        let p = msg!(class(b"NSPasteboard\0"), b"generalPasteboard\0" => Id);
        msg!(p, b"clearContents\0" => i64);
        msg!(p, b"setString:forType:\0" => i8, Id => string(text), Id => string(b"public.utf8-plain-text\0".as_ptr().cast()));
    }
    pub unsafe fn clipboard_get() -> *const c_char {
        if !main_thread() {
            return b"\0".as_ptr().cast();
        }
        let _pool = Pool::new();
        let p = msg!(class(b"NSPasteboard\0"), b"generalPasteboard\0" => Id);
        let s = msg!(p, b"stringForType:\0" => Id, Id => string(b"public.utf8-plain-text\0".as_ptr().cast()));
        let utf8 = if s.is_null() {
            null()
        } else {
            msg!(s, b"UTF8String\0" => *const c_char)
        };
        let owned = if utf8.is_null() {
            CString::default()
        } else {
            CStr::from_ptr(utf8).to_owned()
        };
        REG.with(|r| {
            let mut r = r.borrow_mut();
            r.clipboard = owned;
            r.clipboard.as_ptr()
        })
    }
    pub unsafe fn alert(text: *const c_char, caption: *const c_char) {
        if !main_thread() {
            return;
        }
        let _pool = Pool::new();
        let a = msg!(msg!(class(b"NSAlert\0"), b"alloc\0" => Id), b"init\0" => Id);
        msg!(a, b"setMessageText:\0" => (), Id => string(caption));
        msg!(a, b"setInformativeText:\0" => (), Id => string(text));
        msg!(a, b"addButtonWithTitle:\0" => Id, Id => string(b"OK\0".as_ptr().cast()));
        msg!(a, b"runModal\0" => i64);
        release(a);
    }
}

// Keep exports unconditional and top-level: the Aoxn scanner is not a cfg evaluator.
#[no_mangle]
pub extern "C" fn cocoa_open(title: *const c_char, w: i64, h: i64) -> i64 {
    #[cfg(target_os = "macos")]
    unsafe {
        return mac::open(title, w, h);
    }
    #[cfg(not(target_os = "macos"))]
    {
        0
    }
}
#[no_mangle]
pub extern "C" fn cocoa_alive(handle: i64) -> i64 {
    #[cfg(target_os = "macos")]
    unsafe {
        return mac::alive(handle);
    }
    #[cfg(not(target_os = "macos"))]
    {
        0
    }
}
#[no_mangle]
pub extern "C" fn cocoa_width(handle: i64) -> i64 {
    #[cfg(target_os = "macos")]
    unsafe {
        return mac::dimension(handle, false);
    }
    #[cfg(not(target_os = "macos"))]
    {
        0
    }
}
#[no_mangle]
pub extern "C" fn cocoa_height(handle: i64) -> i64 {
    #[cfg(target_os = "macos")]
    unsafe {
        return mac::dimension(handle, true);
    }
    #[cfg(not(target_os = "macos"))]
    {
        0
    }
}
#[no_mangle]
pub extern "C" fn cocoa_resize(handle: i64, w: i64, h: i64) {
    #[cfg(target_os = "macos")]
    unsafe {
        mac::resize(handle, w, h);
    }
}
#[no_mangle]
pub extern "C" fn cocoa_present(handle: i64) {
    #[cfg(target_os = "macos")]
    unsafe {
        mac::present(handle);
    }
}
#[no_mangle]
pub extern "C" fn cocoa_pump(handle: i64, state: i64, wait_ms: i64) {
    #[cfg(target_os = "macos")]
    unsafe {
        mac::pump(handle, state, wait_ms);
    }
}
#[no_mangle]
pub extern "C" fn cocoa_shape(
    handle: i64,
    kind: i64,
    x: i64,
    y: i64,
    w: i64,
    h: i64,
    fill: i64,
    border: i64,
) {
    #[cfg(target_os = "macos")]
    unsafe {
        mac::shape(handle, kind, x, y, w, h, fill, border);
    }
}
#[no_mangle]
pub extern "C" fn cocoa_text(
    handle: i64,
    x: i64,
    y: i64,
    text: *const c_char,
    color: i64,
    font: i64,
) {
    #[cfg(target_os = "macos")]
    unsafe {
        mac::text(handle, x, y, text, color, font);
    }
}
#[no_mangle]
pub extern "C" fn cocoa_measure(text: *const c_char, font: i64, dimension: i64) -> i64 {
    #[cfg(target_os = "macos")]
    unsafe {
        return mac::measure(text, font, dimension);
    }
    #[cfg(not(target_os = "macos"))]
    {
        0
    }
}
#[no_mangle]
pub extern "C" fn cocoa_clip(handle: i64, l: i64, t: i64, r: i64, b: i64) {
    #[cfg(target_os = "macos")]
    unsafe {
        mac::clip(handle, l, t, r, b);
    }
}
#[no_mangle]
pub extern "C" fn cocoa_unclip(handle: i64) {
    #[cfg(target_os = "macos")]
    unsafe {
        mac::unclip(handle);
    }
}
#[no_mangle]
pub extern "C" fn cocoa_close(handle: i64) {
    #[cfg(target_os = "macos")]
    unsafe {
        mac::close(handle);
    }
}
#[no_mangle]
pub extern "C" fn cocoa_free(handle: i64) {
    #[cfg(target_os = "macos")]
    unsafe {
        mac::free(handle);
    }
}
#[no_mangle]
pub extern "C" fn cocoa_clipboard_set(text: *const c_char) {
    #[cfg(target_os = "macos")]
    unsafe {
        mac::clipboard_set(text);
    }
}
#[no_mangle]
pub extern "C" fn cocoa_clipboard_get() -> *const c_char {
    #[cfg(target_os = "macos")]
    unsafe {
        return mac::clipboard_get();
    }
    #[cfg(not(target_os = "macos"))]
    {
        b"\0".as_ptr().cast()
    }
}
#[no_mangle]
pub extern "C" fn cocoa_alert(text: *const c_char, caption: *const c_char) {
    #[cfg(target_os = "macos")]
    unsafe {
        mac::alert(text, caption);
    }
}
