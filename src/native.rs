//! Native GPU windows and small immediate widgets. No third-party Rust crates.
//! All window, event and OpenGL work stays on the thread that created the window.
use std::{
    ffi::{CStr, CString, c_char, c_int, c_void},
    marker::PhantomData,
    rc::Rc,
    sync::atomic::{AtomicBool, Ordering},
};
unsafe extern "C" {
    fn zgpui_open(
        title: *const c_char,
        width: c_int,
        height: c_int,
        error: *mut c_char,
        capacity: usize,
    ) -> *mut c_void;
    fn zgpui_close(window: *mut c_void);
    fn zgpui_event(window: *mut c_void, event: *mut NativeEvent, timeout: c_int) -> c_int;
    fn zgpui_size(window: *mut c_void, width: *mut c_int, height: *mut c_int);
    fn zgpui_swap(window: *mut c_void);
    fn zgpui_clipboard(window: *mut c_void, text: *const c_char);
    fn zgpui_begin(width: c_int, height: c_int, background: u32);
    fn zgpui_rect(x: f32, y: f32, width: f32, height: f32, radius: f32, color: u32);
    fn zgpui_polygon(points: *const f32, count: usize, color: u32);
    fn zgpui_line(x1: f32, y1: f32, x2: f32, y2: f32, width: f32, color: u32);
    fn zgpui_text(text: *const c_char, x: f32, y: f32, size: c_int, bold: c_int, color: u32)
    -> f32;
    fn zgpui_measure(text: *const c_char, size: c_int, bold: c_int) -> f32;
    fn zgpui_clip(x: c_int, y: c_int, width: c_int, height: c_int);
    fn zgpui_unclip();
    fn zgpui_gpu_info() -> *const c_char;
}
#[repr(C)]
struct NativeEvent {
    kind: c_int,
    x: c_int,
    y: c_int,
    width: c_int,
    height: c_int,
    key: c_int,
    modifiers: c_int,
    text: [c_char; 4096],
}
#[derive(Clone, Debug)]
pub enum Event {
    Move(f32, f32),
    Down(f32, f32),
    Up(f32, f32),
    Key {
        code: i32,
        control: bool,
        shift: bool,
        text: String,
    },
    Text(String),
    Resize,
    Scroll(i32),
    Close,
}
static WINDOW_OPEN: AtomicBool = AtomicBool::new(false);

/// The built-in palette. Custom colors are preserved.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Theme {
    #[default]
    Light,
    Dark,
}
impl Theme {
    pub fn color(self, value: u32) -> u32 {
        if self == Self::Light {
            return value;
        }
        match value {
            0xffffffff => 0x17191dff,
            0xf7f9fcff | 0xf1f3f4ff => 0x202329ff,
            0x202124ff => 0xe8eaedff,
            0x5f6368ff => 0xaeb4bfff,
            0xe8eaedff | 0xe8edf3ff => 0x363b43ff,
            0x1a73e8ff => 0x8ab4f8ff,
            0x1765ccff => 0xa8c7faff,
            0xe8f0feff | 0xf5f8ffff => 0x263650ff,
            0xf0f4faff => 0x2c3038ff,
            0x147d4eff => 0x81c995ff,
            0xe6f4eaff => 0x203c2dff,
            0xb42318ff => 0xf28b82ff,
            0xffe0deff | 0xffeeedff => 0x462b2bff,
            0xeff2f6ff => 0x252930ff,
            0x9aa5b5ff | 0x9aa0a6ff => 0x858c99ff,
            _ => value,
        }
    }
}
pub struct Window {
    theme: Theme,
    raw: *mut c_void,
    _thread: PhantomData<Rc<()>>,
}
fn string(value: &str) -> CString {
    CString::new(value.replace('\0', "")).expect("interior NUL removed")
}
impl Window {
    /// Creates a caller-sized native window. Linux and macOS use this content
    /// size as the initial minimum. Linux derives WM_CLASS from the leading
    /// ASCII identifier in the title (letters, digits, dots, underscores, dashes).
    /// For example, "ztype - document" has instance "ztype" and class "Ztype".
    pub fn new(title: &str, width: i32, height: i32) -> Result<Self, String> {
        if WINDOW_OPEN
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Err("zgpui supports one live window per process".into());
        }
        let title = string(title);
        let mut error = [0 as c_char; 1024];
        let raw = unsafe {
            zgpui_open(
                title.as_ptr(),
                width,
                height,
                error.as_mut_ptr(),
                error.len(),
            )
        };
        if raw.is_null() {
            WINDOW_OPEN.store(false, Ordering::Release);
            return Err(unsafe { CStr::from_ptr(error.as_ptr()) }
                .to_string_lossy()
                .into_owned());
        }
        Ok(Self {
            theme: Theme::Light,
            raw,
            _thread: PhantomData,
        })
    }
    pub fn set_theme(&mut self, theme: Theme) {
        self.theme = theme;
    }
    pub fn size(&self) -> (f32, f32) {
        let (mut width, mut height) = (0, 0);
        unsafe { zgpui_size(self.raw, &mut width, &mut height) };
        (width as f32, height as f32)
    }
    pub fn event(&mut self, timeout_ms: i32) -> Option<Event> {
        let mut raw = NativeEvent {
            kind: 0,
            x: 0,
            y: 0,
            width: 0,
            height: 0,
            key: 0,
            modifiers: 0,
            text: [0; 4096],
        };
        if unsafe { zgpui_event(self.raw, &mut raw, timeout_ms.clamp(0, 1000)) } == 0 {
            return None;
        }
        let text = unsafe { CStr::from_ptr(raw.text.as_ptr()) }
            .to_string_lossy()
            .into_owned();
        Some(match raw.kind {
            1 => Event::Move(raw.x as f32, raw.y as f32),
            2 => Event::Down(raw.x as f32, raw.y as f32),
            3 => Event::Up(raw.x as f32, raw.y as f32),
            4 => Event::Key {
                code: raw.key,
                control: raw.modifiers & 1 != 0,
                shift: raw.modifiers & 2 != 0,
                text,
            },
            5 => Event::Text(text),
            6 => Event::Resize,
            7 => Event::Close,
            8 => Event::Scroll(raw.key),
            _ => return None,
        })
    }
    pub fn begin(&mut self, background: u32) -> Canvas<'_> {
        let (width, height) = self.size();
        unsafe { zgpui_begin(width as i32, height as i32, self.theme.color(background)) };
        Canvas {
            theme: self.theme,
            width,
            height,
            _window: PhantomData,
        }
    }
    pub fn present(&mut self) {
        unsafe { zgpui_swap(self.raw) }
    }
    pub fn copy(&mut self, text: &str) {
        let text = string(text);
        unsafe { zgpui_clipboard(self.raw, text.as_ptr()) }
    }
    pub fn gpu(&self) -> String {
        unsafe { CStr::from_ptr(zgpui_gpu_info()) }
            .to_string_lossy()
            .into_owned()
    }
}
impl Drop for Window {
    fn drop(&mut self) {
        unsafe { zgpui_close(self.raw) };
        WINDOW_OPEN.store(false, Ordering::Release);
    }
}
#[derive(Clone, Copy, Debug, Default)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}
impl Rect {
    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }
    pub fn contains(self, x: f32, y: f32) -> bool {
        x >= self.x && x < self.x + self.w && y >= self.y && y < self.y + self.h
    }
    pub fn inset(self, amount: f32) -> Self {
        Self::new(
            self.x + amount,
            self.y + amount,
            (self.w - amount * 2.).max(0.),
            (self.h - amount * 2.).max(0.),
        )
    }
}
pub struct Canvas<'a> {
    pub theme: Theme,
    pub width: f32,
    pub height: f32,
    _window: PhantomData<&'a mut Window>,
}
impl Canvas<'_> {
    pub fn rect(&mut self, rect: Rect, radius: f32, color: u32) {
        unsafe {
            zgpui_rect(
                rect.x,
                rect.y,
                rect.w,
                rect.h,
                radius,
                self.theme.color(color),
            )
        }
    }
    pub fn text(&mut self, text: &str, x: f32, y: f32, size: i32, bold: bool, color: u32) -> f32 {
        let text = string(text);
        unsafe {
            zgpui_text(
                text.as_ptr(),
                x.round(),
                y.round(),
                size,
                bold as i32,
                self.theme.color(color),
            )
        }
    }
    /// Draw a convex polygon with at least three points.
    pub fn polygon(&mut self, points: &[(f32, f32)], color: u32) {
        if points.len() < 3 || points.len() > 4096 {
            return;
        }
        let mut coordinates = Vec::with_capacity(points.len() * 2);
        for &(x, y) in points {
            coordinates.extend([x, y]);
        }
        unsafe { zgpui_polygon(coordinates.as_ptr(), points.len(), self.theme.color(color)) }
    }
    pub fn line(&mut self, from: (f32, f32), to: (f32, f32), width: f32, color: u32) {
        unsafe {
            zgpui_line(
                from.0,
                from.1,
                to.0,
                to.1,
                width.max(0.),
                self.theme.color(color),
            )
        }
    }
    pub fn measure(&self, text: &str, size: i32, bold: bool) -> f32 {
        let text = string(text);
        unsafe { zgpui_measure(text.as_ptr(), size, bold as i32) }
    }
    pub fn clip(&mut self, rect: Rect) {
        unsafe { zgpui_clip(rect.x as i32, rect.y as i32, rect.w as i32, rect.h as i32) }
    }
    pub fn unclip(&mut self) {
        unsafe { zgpui_unclip() }
    }
    #[allow(
        clippy::too_many_arguments,
        reason = "drawing primitives carry geometry and style"
    )]
    pub fn ellipsis(
        &mut self,
        text: &str,
        x: f32,
        y: f32,
        max_width: f32,
        size: i32,
        bold: bool,
        color: u32,
    ) {
        if self.measure(text, size, bold) <= max_width {
            self.text(text, x, y, size, bold, color);
            return;
        }
        let mut cut = text.to_owned();
        while !cut.is_empty() && self.measure(&(cut.clone() + "…"), size, bold) > max_width {
            cut.pop();
        }
        self.text(&(cut + "…"), x, y, size, bold, color);
    }
}
impl Drop for Canvas<'_> {
    fn drop(&mut self) {
        unsafe { zgpui_unclip() }
    }
}
pub mod colors {
    pub const BG: u32 = 0xf7f9fcff;
    pub const SURFACE: u32 = 0xffffffff;
    pub const TEXT: u32 = 0x202124ff;
    pub const MUTED: u32 = 0x5f6368ff;
    pub const LINE: u32 = 0xe8eaedff;
    pub const BLUE: u32 = 0x1a73e8ff;
    pub const BLUE_PALE: u32 = 0xe8f0feff;
    pub const GREEN: u32 = 0x147d4eff;
    pub const RED: u32 = 0xb42318ff;
}
#[derive(Clone, Copy)]
pub enum ButtonStyle {
    Primary,
    Secondary,
    Quiet,
    Danger,
}
#[derive(Default)]
pub struct Ui {
    mouse: (f32, f32),
    down: bool,
    released: bool,
    pressed: bool,
    event: Option<Event>,
    active: Option<u64>,
    focus: Option<u64>,
    order: Vec<u64>,
    previous: Vec<u64>,
    boxes: Vec<(u64, Rect)>,
    previous_boxes: Vec<(u64, Rect)>,
    cursor: usize,
    select_all: bool,
    pub clipboard: Option<String>,
}
impl Ui {
    /// Update pointer position and redraw only when its widget target changes.
    pub fn pointer_changed(&mut self, x: f32, y: f32) -> bool {
        let hit = |x, y| {
            self.previous_boxes
                .iter()
                .rev()
                .find(|(_, r)| r.contains(x, y))
                .map(|(id, _)| *id)
        };
        let changed = hit(self.mouse.0, self.mouse.1) != hit(x, y);
        self.mouse = (x, y);
        changed
    }
    pub fn begin(&mut self, event: Option<Event>) {
        self.released = false;
        self.pressed = false;
        match &event {
            Some(Event::Move(x, y)) => self.mouse = (*x, *y),
            Some(Event::Down(x, y)) => {
                self.mouse = (*x, *y);
                self.down = true;
                self.pressed = true;
            }
            Some(Event::Up(x, y)) => {
                self.mouse = (*x, *y);
                self.down = false;
                self.released = true;
            }
            Some(Event::Key { code: 9, shift, .. }) if !self.previous.is_empty() => {
                let pos = self
                    .focus
                    .and_then(|id| self.previous.iter().position(|v| *v == id));
                let next = if *shift {
                    (pos.unwrap_or(0) + self.previous.len() - 1) % self.previous.len()
                } else {
                    pos.map_or(0, |i| (i + 1) % self.previous.len())
                };
                self.focus = Some(self.previous[next]);
                self.cursor = usize::MAX;
                self.select_all = false;
            }
            _ => {}
        }
        self.event = event;
        self.order.clear();
        self.boxes.clear();
    }
    pub fn end(&mut self) {
        if self.released {
            self.active = None;
        }
        std::mem::swap(&mut self.order, &mut self.previous);
        std::mem::swap(&mut self.boxes, &mut self.previous_boxes);
        self.event = None;
    }
    fn register(&mut self, id: u64, rect: Rect) -> bool {
        self.order.push(id);
        self.boxes.push((id, rect));
        let hover = rect.contains(self.mouse.0, self.mouse.1);
        if hover && self.pressed {
            self.active = Some(id);
            if self.focus != Some(id) {
                self.cursor = usize::MAX;
                self.select_all = false;
            }
            self.focus = Some(id);
        }
        hover
    }
    pub fn button(
        &mut self,
        canvas: &mut Canvas<'_>,
        id: u64,
        rect: Rect,
        label: &str,
        style: ButtonStyle,
        enabled: bool,
    ) -> bool {
        use colors::*;
        let hover = if enabled {
            self.register(id, rect)
        } else {
            false
        };
        let focused = self.focus == Some(id) && enabled;
        let clicked = enabled
            && ((hover && self.released && self.active == Some(id))
                || (focused && matches!(self.event, Some(Event::Key { code: 13 | 32, .. }))));
        let (fill, ink) = if !enabled {
            (0xeff2f6ff, 0x9aa5b5ff)
        } else {
            match style {
                ButtonStyle::Primary => (if hover { 0x1765ccff } else { BLUE }, SURFACE),
                ButtonStyle::Danger => (if hover { 0xffe0deff } else { 0xffeeedff }, RED),
                ButtonStyle::Quiet => (
                    if hover || focused {
                        BLUE_PALE
                    } else {
                        0xffffff00
                    },
                    if focused { BLUE } else { TEXT },
                ),
                ButtonStyle::Secondary => (if hover { 0xf0f4faff } else { SURFACE }, TEXT),
            }
        };
        if matches!(style, ButtonStyle::Secondary) || focused {
            canvas.rect(rect, 12., if focused { BLUE } else { LINE });
            canvas.rect(rect.inset(if focused { 2. } else { 1. }), 11., fill);
        } else {
            canvas.rect(rect, 12., fill);
        }
        let width = canvas.measure(label, 15, false);
        canvas.text(
            label,
            rect.x + (rect.w - width) / 2.,
            rect.y + (rect.h - 19.) / 2.,
            15,
            false,
            ink,
        );
        clicked
    }
    #[allow(
        clippy::too_many_arguments,
        reason = "widget placement and state are explicit"
    )]
    pub fn input(
        &mut self,
        canvas: &mut Canvas<'_>,
        id: u64,
        rect: Rect,
        value: &mut String,
        placeholder: &str,
        secret: bool,
        enabled: bool,
    ) -> bool {
        use colors::*;
        if enabled {
            self.register(id, rect);
        }
        let focused = self.focus == Some(id) && enabled;
        let mut changed = false;
        if focused {
            self.cursor = self.cursor.min(value.len());
            while !value.is_char_boundary(self.cursor) {
                self.cursor -= 1;
            }
            if self.pressed && rect.contains(self.mouse.0, self.mouse.1) {
                self.cursor = value.len();
                self.select_all = false;
            }
            match self.event.clone() {
                Some(Event::Key {
                    code,
                    control,
                    shift: _,
                    text,
                }) => {
                    if control {
                        match code {
                            97 | 65 => self.select_all = true,
                            99 | 67 => {
                                if !secret {
                                    self.clipboard = Some(value.clone());
                                }
                            }
                            120 | 88 => {
                                if !secret {
                                    self.clipboard = Some(value.clone());
                                }
                                value.clear();
                                self.cursor = 0;
                                self.select_all = false;
                                changed = true;
                            }
                            _ => {}
                        }
                    } else {
                        match code {
                            8 => {
                                if self.select_all {
                                    value.clear();
                                    self.cursor = 0;
                                } else if self.cursor > 0 {
                                    let previous = value[..self.cursor]
                                        .char_indices()
                                        .last()
                                        .map_or(0, |(i, _)| i);
                                    value.replace_range(previous..self.cursor, "");
                                    self.cursor = previous;
                                }
                                self.select_all = false;
                                changed = true;
                            }
                            127 => {
                                if self.select_all {
                                    value.clear();
                                    self.cursor = 0;
                                } else if self.cursor < value.len() {
                                    let end = self.cursor
                                        + value[self.cursor..].chars().next().unwrap().len_utf8();
                                    value.replace_range(self.cursor..end, "");
                                }
                                self.select_all = false;
                                changed = true;
                            }
                            1001 => {
                                self.cursor = value[..self.cursor]
                                    .char_indices()
                                    .last()
                                    .map_or(0, |(i, _)| i);
                                self.select_all = false;
                            }
                            1002 => {
                                if let Some(c) = value[self.cursor..].chars().next() {
                                    self.cursor += c.len_utf8();
                                }
                                self.select_all = false;
                            }
                            1005 => {
                                self.cursor = 0;
                                self.select_all = false;
                            }
                            1006 => {
                                self.cursor = value.len();
                                self.select_all = false;
                            }
                            9 | 13 | 27 | 1003 | 1004 => {}
                            _ => changed |= self.insert(value, &text),
                        }
                    }
                }
                Some(Event::Text(text)) => changed |= self.insert(value, &text),
                _ => {}
            }
        }
        canvas.rect(rect, 12., if focused { BLUE } else { LINE });
        canvas.rect(
            rect.inset(if focused { 2. } else { 1. }),
            7.,
            if enabled { SURFACE } else { 0xf2f4f7ff },
        );
        let display = if secret {
            "•".repeat(value.chars().count())
        } else {
            value.clone()
        };
        let prefix = if !focused {
            String::new()
        } else if secret {
            "•".repeat(value[..self.cursor.min(value.len())].chars().count())
        } else {
            value
                .get(..self.cursor.min(value.len()))
                .unwrap_or("")
                .into()
        };
        let cursor_width = canvas.measure(&prefix, 15, false);
        let scroll = if focused {
            (cursor_width - rect.w + 30.).max(0.)
        } else {
            0.
        };
        canvas.clip(rect.inset(7.));
        if focused && self.select_all {
            canvas.rect(
                Rect::new(
                    rect.x + 10. - scroll,
                    rect.y + 8.,
                    canvas.measure(&display, 15, false),
                    22.,
                ),
                2.,
                BLUE_PALE,
            );
        }
        canvas.text(
            if value.is_empty() {
                placeholder
            } else {
                &display
            },
            rect.x + 11. - scroll,
            rect.y + 11.,
            15,
            false,
            if value.is_empty() { MUTED } else { TEXT },
        );
        if focused {
            canvas.rect(
                Rect::new(rect.x + 11. + cursor_width - scroll, rect.y + 9., 1.4, 22.),
                0.,
                BLUE,
            );
        }
        canvas.unclip();
        changed
    }
    fn insert(&mut self, value: &mut String, text: &str) -> bool {
        let filtered: String = text.chars().filter(|c| !c.is_control()).collect();
        if filtered.is_empty() || value.len() + filtered.len() > 4096 {
            return false;
        }
        if self.select_all {
            value.clear();
            self.cursor = 0;
            self.select_all = false;
        }
        self.cursor = self.cursor.min(value.len());
        value.insert_str(self.cursor, &filtered);
        self.cursor += filtered.len();
        true
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unicode_input_preserves_boundaries() {
        let mut ui = Ui::default();
        let mut value = "模型".to_string();
        ui.cursor = value.len();
        assert!(ui.insert(&mut value, " a\nb"));
        assert_eq!(value, "模型 ab");
        ui.select_all = true;
        assert!(ui.insert(&mut value, "新"));
        assert_eq!(value, "新");
    }
    #[test]
    fn bounds_are_exclusive() {
        let r = Rect::new(10., 10., 20., 20.);
        assert!(r.contains(10., 10.));
        assert!(!r.contains(30., 20.));
    }
}
