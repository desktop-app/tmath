//! The C interface of tmath, declared in `include/tmath.h`.
//!
//! A formula is laid out once, with text runs measured through a callback,
//! and the handle keeps its metrics and the display list `TeXRender::draw`
//! would produce until it is freed, so the C++ side can measure now and
//! paint later. The engine itself has no unsafe code; everything unsafe in
//! tmath is in the `abi` module below, which turns C pointers into slices
//! and handles. Every entry point catches panics.

use std::ffi::c_void;
use std::panic::{AssertUnwindSafe, catch_unwind};

use tmath::boxes::Measure;
use tmath::draw::{Op, Rect, TextFont};
use tmath::error::ErrorKind;

/// The formula was laid out.
pub const TMATH_OK: i32 = 0;
/// The formula was refused, as `LaTeX::parse` refuses it by throwing.
pub const TMATH_REFUSED: i32 = 1;
/// The arguments were invalid.
pub const TMATH_INVALID: i32 = 2;
/// The engine failed in a way it never should: a bug.
pub const TMATH_INTERNAL: i32 = 3;

pub const TMATH_OP_SET_COLOR: i32 = 0;
pub const TMATH_OP_SET_STROKE: i32 = 1;
pub const TMATH_OP_SET_STROKE_WIDTH: i32 = 2;
pub const TMATH_OP_SET_FONT: i32 = 3;
pub const TMATH_OP_TRANSLATE: i32 = 4;
pub const TMATH_OP_SCALE: i32 = 5;
pub const TMATH_OP_ROTATE: i32 = 6;
pub const TMATH_OP_ROTATE_AT: i32 = 7;
pub const TMATH_OP_RESET: i32 = 8;
pub const TMATH_OP_DRAW_CHAR: i32 = 9;
pub const TMATH_OP_DRAW_TEXT: i32 = 10;
pub const TMATH_OP_DRAW_LINE: i32 = 11;
pub const TMATH_OP_DRAW_RECT: i32 = 12;
pub const TMATH_OP_FILL_RECT: i32 = 13;
pub const TMATH_OP_DRAW_ROUND_RECT: i32 = 14;
pub const TMATH_OP_FILL_ROUND_RECT: i32 = 15;
pub const TMATH_OP_LAYOUT: i32 = 16;

/// `TMathRect`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TMathRect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// `TMathMeasureText`: the bounds of a text run, as `TextLayout::getBounds`
/// gives them for a font of this family, style and integer pixel size.
pub type TMathMeasureText = Option<
    unsafe extern "C" fn(
        context: *mut c_void,
        family: *const u8,
        family_len: usize,
        style: i32,
        pixel_size: i32,
        text: *const u16,
        text_len: usize,
    ) -> TMathRect,
>;

/// `TMathOp`: one `Graphics2D` call. Pointers stay valid while the formula
/// they came from lives.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct TMathOp {
    pub kind: i32,
    pub color: u32,
    pub ints: [i32; 2],
    pub values: [f32; 6],
    pub text: *const u16,
    pub text_len: usize,
    pub family: *const u8,
    pub family_len: usize,
    pub style: i32,
    pub pixel_size: i32,
}

impl Default for TMathOp {
    fn default() -> Self {
        Self {
            kind: -1,
            color: 0,
            ints: [0; 2],
            values: [0.0; 6],
            text: std::ptr::null(),
            text_len: 0,
            family: std::ptr::null(),
            family_len: 0,
            style: 0,
            pixel_size: 0,
        }
    }
}

/// `TMathFormula`: a laid-out formula, or why there is none.
pub struct TMathFormula {
    status: i32,
    /// UTF-8 with a NUL after it.
    error: Vec<u8>,
    width: i32,
    height: i32,
    depth: i32,
    baseline: f32,
    insets: [i32; 4],
    ops: Vec<Op>,
}

impl TMathFormula {
    fn failed(status: i32, message: &str) -> Self {
        let mut error = message.as_bytes().to_vec();
        error.push(0);
        Self {
            status,
            error,
            width: 0,
            height: 0,
            depth: 0,
            baseline: 0.0,
            insets: [0; 4],
            ops: Vec::new(),
        }
    }

    /// Lays `tex` out, measuring text runs with `measure`.
    fn lay_out(
        tex: &[u16],
        width: i32,
        text_size: f32,
        line_space: f32,
        foreground: u32,
        measure: &mut dyn Measure,
    ) -> Self {
        let result = catch_unwind(AssertUnwindSafe(|| {
            tmath::layout(tex, width, text_size, line_space, foreground, measure).map(|laid| {
                let insets = laid.render.insets;
                Self {
                    status: TMATH_OK,
                    error: vec![0],
                    width: laid.width(),
                    height: laid.height(),
                    depth: laid.depth(),
                    baseline: laid.baseline(),
                    insets: [insets.left, insets.top, insets.right, insets.bottom],
                    ops: laid.draw(),
                }
            })
        }));
        match result {
            Ok(Ok(formula)) => formula,
            Ok(Err(e)) => {
                let status = if e.kind == ErrorKind::Unsupported {
                    TMATH_INTERNAL
                } else {
                    TMATH_REFUSED
                };
                Self::failed(status, &e.message)
            }
            Err(_) => Self::failed(TMATH_INTERNAL, "The formula engine failed."),
        }
    }

    /// The op at `index` in its C form.
    fn op(&self, index: usize) -> Option<TMathOp> {
        let op = self.ops.get(index)?;
        let mut out = TMathOp::default();
        let mut values = |v: &[f32]| out.values[..v.len()].copy_from_slice(v);
        let kind = match op {
            Op::SetColor(c) => {
                out.color = *c;
                TMATH_OP_SET_COLOR
            }
            Op::SetStroke(s) => {
                values(&[s.line_width, s.miter_limit]);
                out.ints = [s.cap, s.join];
                TMATH_OP_SET_STROKE
            }
            Op::SetStrokeWidth(w) => {
                values(&[*w]);
                TMATH_OP_SET_STROKE_WIDTH
            }
            Op::SetFont(id) => {
                out.ints[0] = *id;
                TMATH_OP_SET_FONT
            }
            Op::Translate(x, y) => {
                values(&[*x, *y]);
                TMATH_OP_TRANSLATE
            }
            Op::Scale(x, y) => {
                values(&[*x, *y]);
                TMATH_OP_SCALE
            }
            Op::Rotate(a) => {
                values(&[*a]);
                TMATH_OP_ROTATE
            }
            Op::RotateAt(a, x, y) => {
                values(&[*a, *x, *y]);
                TMATH_OP_ROTATE_AT
            }
            Op::Reset => TMATH_OP_RESET,
            Op::DrawChar(c, x, y) => {
                out.ints[0] = i32::from(*c);
                values(&[*x, *y]);
                TMATH_OP_DRAW_CHAR
            }
            Op::DrawText(text, x, y) => {
                values(&[*x, *y]);
                out.text = text.as_ptr();
                out.text_len = text.len();
                TMATH_OP_DRAW_TEXT
            }
            Op::DrawLine(a, b, c, d) => {
                values(&[*a, *b, *c, *d]);
                TMATH_OP_DRAW_LINE
            }
            Op::DrawRect(a, b, c, d) => {
                values(&[*a, *b, *c, *d]);
                TMATH_OP_DRAW_RECT
            }
            Op::FillRect(a, b, c, d) => {
                values(&[*a, *b, *c, *d]);
                TMATH_OP_FILL_RECT
            }
            Op::DrawRoundRect(a, b, c, d, e, f) => {
                values(&[*a, *b, *c, *d, *e, *f]);
                TMATH_OP_DRAW_ROUND_RECT
            }
            Op::FillRoundRect(a, b, c, d, e, f) => {
                values(&[*a, *b, *c, *d, *e, *f]);
                TMATH_OP_FILL_ROUND_RECT
            }
            Op::Layout(font, text, x, y) => {
                values(&[*x, *y]);
                out.text = text.as_ptr();
                out.text_len = text.len();
                out.family = font.family.as_ptr();
                out.family_len = font.family.len();
                out.style = font.style;
                out.pixel_size = font.pixel_size;
                TMATH_OP_LAYOUT
            }
        };
        out.kind = kind;
        Some(out)
    }
}

/// Text measurement through the caller's function.
struct CallbackMeasure {
    measure: abi::MeasureFn,
    context: *mut c_void,
}

impl Measure for CallbackMeasure {
    fn measure(&mut self, font: &TextFont, text: &[u16]) -> Rect {
        let r = abi::call_measure(self.measure, self.context, font, text);
        Rect {
            x: r.x,
            y: r.y,
            w: r.w,
            h: r.h,
        }
    }
}

/// Where C pointers become Rust values. Each function states what it
/// requires of its caller in `include/tmath.h`.
#[allow(unsafe_code)]
mod abi {
    use super::*;

    pub type MeasureFn = unsafe extern "C" fn(
        *mut c_void,
        *const u8,
        usize,
        i32,
        i32,
        *const u16,
        usize,
    ) -> TMathRect;

    pub fn call_measure(
        measure: MeasureFn,
        context: *mut c_void,
        font: &TextFont,
        text: &[u16],
    ) -> TMathRect {
        // SAFETY: `tmath_layout`'s caller passed a function with this
        // signature that reads `family_len` bytes and `text_len` units, which
        // both slices have, and does not unwind into Rust.
        unsafe {
            measure(
                context,
                font.family.as_ptr(),
                font.family.len(),
                font.style,
                font.pixel_size,
                text.as_ptr(),
                text.len(),
            )
        }
    }

    /// `tmath_layout`: lays a formula out. Never returns NULL: a formula that
    /// is refused comes back with its status and message.
    ///
    /// # Safety
    ///
    /// `tex` points to `tex_len` UTF-16 units, or is NULL when `tex_len` is
    /// 0, and `measure` is a function as `TMathMeasureText` describes it.
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn tmath_layout(
        tex: *const u16,
        tex_len: usize,
        width: i32,
        text_size: f32,
        line_space: f32,
        foreground: u32,
        measure: TMathMeasureText,
        context: *mut c_void,
    ) -> *mut TMathFormula {
        let formula = match (measure, tex.is_null() && tex_len > 0) {
            (Some(measure), false) => {
                let tex: &[u16] = if tex_len == 0 {
                    &[]
                } else {
                    // SAFETY: the caller promises `tex_len` units at `tex`.
                    unsafe { std::slice::from_raw_parts(tex, tex_len) }
                };
                let mut callback = CallbackMeasure { measure, context };
                TMathFormula::lay_out(tex, width, text_size, line_space, foreground, &mut callback)
            }
            _ => TMathFormula::failed(TMATH_INVALID, "Invalid arguments."),
        };
        Box::into_raw(Box::new(formula))
    }

    fn formula<'a>(formula: *const TMathFormula) -> Option<&'a TMathFormula> {
        // SAFETY: every function taking a formula requires NULL or a handle
        // `tmath_layout` returned and `tmath_free` has not freed.
        unsafe { formula.as_ref() }
    }

    /// # Safety
    ///
    /// `formula` is NULL or a live handle from `tmath_layout`.
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn tmath_free(formula: *mut TMathFormula) {
        if !formula.is_null() {
            // SAFETY: the caller passes a live handle, which came from
            // `Box::into_raw` in `tmath_layout`, once.
            drop(unsafe { Box::from_raw(formula) });
        }
    }

    /// # Safety
    ///
    /// `formula` is NULL or a live handle from `tmath_layout`.
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn tmath_status(formula: *const TMathFormula) -> i32 {
        super::abi::formula(formula).map_or(TMATH_INVALID, |f| f.status)
    }

    /// # Safety
    ///
    /// `formula` is NULL or a live handle from `tmath_layout`, and `len` is
    /// NULL or writable.
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn tmath_error(
        formula: *const TMathFormula,
        len: *mut usize,
    ) -> *const u8 {
        let bytes: &[u8] = formula_ref(formula).map_or(b"\0", |f| &f.error);
        if !len.is_null() {
            // SAFETY: the caller passes NULL or a writable `size_t`.
            unsafe { *len = bytes.len() - 1 };
        }
        bytes.as_ptr()
    }

    fn formula_ref<'a>(formula: *const TMathFormula) -> Option<&'a TMathFormula> {
        super::abi::formula(formula)
    }

    /// # Safety
    ///
    /// `formula` is NULL or a live handle from `tmath_layout`.
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn tmath_width(formula: *const TMathFormula) -> i32 {
        formula_ref(formula).map_or(0, |f| f.width)
    }

    /// # Safety
    ///
    /// `formula` is NULL or a live handle from `tmath_layout`.
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn tmath_height(formula: *const TMathFormula) -> i32 {
        formula_ref(formula).map_or(0, |f| f.height)
    }

    /// # Safety
    ///
    /// `formula` is NULL or a live handle from `tmath_layout`.
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn tmath_depth(formula: *const TMathFormula) -> i32 {
        formula_ref(formula).map_or(0, |f| f.depth)
    }

    /// # Safety
    ///
    /// `formula` is NULL or a live handle from `tmath_layout`.
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn tmath_baseline(formula: *const TMathFormula) -> f32 {
        formula_ref(formula).map_or(0.0, |f| f.baseline)
    }

    /// # Safety
    ///
    /// `formula` is NULL or a live handle from `tmath_layout`, and `insets`
    /// is NULL or points to four writable `int32_t`: left, top, right,
    /// bottom.
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn tmath_insets(formula: *const TMathFormula, insets: *mut i32) {
        let values = formula_ref(formula).map_or([0; 4], |f| f.insets);
        if !insets.is_null() {
            // SAFETY: the caller passes room for four values.
            unsafe { std::ptr::copy_nonoverlapping(values.as_ptr(), insets, 4) };
        }
    }

    /// # Safety
    ///
    /// `formula` is NULL or a live handle from `tmath_layout`.
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn tmath_op_count(formula: *const TMathFormula) -> usize {
        formula_ref(formula).map_or(0, |f| f.ops.len())
    }

    /// Fills `op` with the op at `index`; returns 0 and leaves it alone when
    /// there is none.
    ///
    /// # Safety
    ///
    /// `formula` is NULL or a live handle from `tmath_layout`, and `op` is
    /// NULL or writable.
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn tmath_op(
        formula: *const TMathFormula,
        index: usize,
        op: *mut TMathOp,
    ) -> i32 {
        let Some(value) = formula_ref(formula).and_then(|f| f.op(index)) else {
            return 0;
        };
        if op.is_null() {
            return 0;
        }
        // SAFETY: the caller passes a writable `TMathOp`.
        unsafe { op.write(value) };
        1
    }
}

#[cfg(test)]
#[allow(unsafe_code)]
mod tests {
    use super::abi::*;
    use super::*;

    /// The arithmetic the oracle's recording backend measures with.
    unsafe extern "C" fn measure(
        _context: *mut c_void,
        _family: *const u8,
        _family_len: usize,
        _style: i32,
        pixel_size: i32,
        _text: *const u16,
        text_len: usize,
    ) -> TMathRect {
        let size = pixel_size as f32;
        TMathRect {
            x: 0.0,
            y: -0.75 * size,
            w: text_len as f32 * 0.5 * size,
            h: size,
        }
    }

    fn lay_out(tex: &str) -> *mut TMathFormula {
        let units: Vec<u16> = tex.encode_utf16().collect();
        // SAFETY: `units` holds `units.len()` units and `measure` matches.
        unsafe {
            tmath_layout(
                units.as_ptr(),
                units.len(),
                0,
                1024.0,
                256.0,
                0xFFFF_FFFF,
                Some(measure),
                std::ptr::null_mut(),
            )
        }
    }

    #[test]
    fn lays_out_and_lists_ops() {
        let formula = lay_out(r"\frac{a}{b}+\text{привет}");
        // SAFETY: `formula` is live until the `tmath_free` below.
        unsafe {
            assert_eq!(tmath_status(formula), TMATH_OK);
            assert!(tmath_width(formula) > 0 && tmath_height(formula) > 0);
            let count = tmath_op_count(formula);
            assert!(count > 0);
            let mut op = TMathOp::default();
            let mut kinds = Vec::new();
            for i in 0..count {
                assert_eq!(tmath_op(formula, i, &mut op), 1);
                kinds.push(op.kind);
                if op.kind == TMATH_OP_LAYOUT {
                    let text = std::slice::from_raw_parts(op.text, op.text_len);
                    assert_eq!(String::from_utf16_lossy(text), "привет");
                }
            }
            assert!(kinds.contains(&TMATH_OP_DRAW_CHAR) && kinds.contains(&TMATH_OP_LAYOUT));
            assert_eq!(tmath_op(formula, count, &mut op), 0);
            let mut insets = [7; 4];
            tmath_insets(formula, insets.as_mut_ptr());
            assert_eq!(insets, [0; 4]);
            tmath_free(formula);
        }
    }

    #[test]
    fn refuses_with_a_message() {
        let formula = lay_out(
            "{{{{{{{{{{{{{{{{{{{{{{{{{{{{{{{{{{{{{{{{{{{{{{{{{{x}}}}}}}}}}}}}}}}}}}}}}}}}}}}}}}}}}}}}}}}}}}}}}}}}}",
        );
        // SAFETY: `formula` is live until the `tmath_free` below.
        unsafe {
            assert_eq!(tmath_status(formula), TMATH_REFUSED);
            let mut len = 0;
            let error = tmath_error(formula, &mut len);
            let text = std::str::from_utf8(std::slice::from_raw_parts(error, len)).unwrap_or("");
            assert!(text.contains("nesting"), "{text}");
            assert_eq!(*error.add(len), 0);
            assert_eq!(tmath_op_count(formula), 0);
            tmath_free(formula);
        }
    }

    #[test]
    fn checks_its_arguments() {
        // SAFETY: NULL with length 0 is allowed, NULL handles are allowed.
        unsafe {
            let empty = tmath_layout(
                std::ptr::null(),
                0,
                0,
                1024.0,
                0.0,
                0,
                Some(measure),
                std::ptr::null_mut(),
            );
            assert_eq!(tmath_status(empty), TMATH_OK);
            tmath_free(empty);
            let missing = tmath_layout(
                std::ptr::null(),
                3,
                0,
                1024.0,
                0.0,
                0,
                Some(measure),
                std::ptr::null_mut(),
            );
            assert_eq!(tmath_status(missing), TMATH_INVALID);
            tmath_free(missing);
            let units = [u16::from(b'x')];
            let unmeasured = tmath_layout(
                units.as_ptr(),
                1,
                0,
                1024.0,
                0.0,
                0,
                None,
                std::ptr::null_mut(),
            );
            assert_eq!(tmath_status(unmeasured), TMATH_INVALID);
            tmath_free(unmeasured);
            assert_eq!(tmath_status(std::ptr::null()), TMATH_INVALID);
            assert_eq!(tmath_op_count(std::ptr::null()), 0);
            tmath_free(std::ptr::null_mut());
        }
    }
}
