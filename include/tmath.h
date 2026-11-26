/*
 * tmath: LaTeX math layout in safe Rust, a port of MicroTeX.
 *
 * A formula is laid out once, measuring its text runs through a callback,
 * and the handle keeps the metrics and the display list TeXRender::draw()
 * would produce, so a caller can measure now and paint later by replaying
 * the list (see replay/tmath_replay.h).
 */
#ifndef TMATH_H
#define TMATH_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct TMathFormula TMathFormula;

enum TMathStatus {
  /* Laid out. */
  TMATH_OK = 0,
  /* Refused, as LaTeX::parse() refuses a formula by throwing. */
  TMATH_REFUSED = 1,
  /* Invalid arguments. */
  TMATH_INVALID = 2,
  /* An engine failure that should never happen. */
  TMATH_INTERNAL = 3,
};

typedef struct TMathRect {
  float x;
  float y;
  float w;
  float h;
} TMathRect;

/*
 * The bounds of a text run, as TextLayout::getBounds() gives them for a
 * font of this family (UTF-8, not NUL-terminated), style (1 bold, 2 italic)
 * and integer pixel size. The text is UTF-16. The function must not throw.
 */
typedef TMathRect (*TMathMeasureText)(
  void *context,
  const uint8_t *family,
  size_t family_len,
  int32_t style,
  int32_t pixel_size,
  const uint16_t *text,
  size_t text_len);

/*
 * LaTeX::parse(tex, width, text_size, line_space, foreground) as tdesktop
 * calls it, with tex as UTF-16 (NULL when tex_len is 0). Never returns NULL:
 * check tmath_status(). Free the result with tmath_free().
 */
TMathFormula *tmath_layout(
  const uint16_t *tex,
  size_t tex_len,
  int32_t width,
  float text_size,
  float line_space,
  uint32_t foreground,
  TMathMeasureText measure,
  void *context);

void tmath_free(TMathFormula *formula);

int32_t tmath_status(const TMathFormula *formula);

/* Why the formula was refused, UTF-8 and NUL-terminated; len may be NULL. */
const uint8_t *tmath_error(const TMathFormula *formula, size_t *len);

/* TeXRender::getWidth(), getHeight(), getDepth(), getBaseline(). */
int32_t tmath_width(const TMathFormula *formula);
int32_t tmath_height(const TMathFormula *formula);
int32_t tmath_depth(const TMathFormula *formula);
float tmath_baseline(const TMathFormula *formula);

/* TeXRender::getInsets(): left, top, right, bottom. */
void tmath_insets(const TMathFormula *formula, int32_t insets[4]);

/* The Graphics2D calls of TeXRender::draw(g2, 0, 0), in order. */
enum TMathOpKind {
  TMATH_OP_SET_COLOR = 0,        /* color */
  TMATH_OP_SET_STROKE = 1,       /* values: width, miter limit; ints: cap, join */
  TMATH_OP_SET_STROKE_WIDTH = 2, /* values: width */
  TMATH_OP_SET_FONT = 3,         /* ints: TeX font id, for FontInfo::getFont() */
  TMATH_OP_TRANSLATE = 4,        /* values: dx, dy */
  TMATH_OP_SCALE = 5,            /* values: sx, sy */
  TMATH_OP_ROTATE = 6,           /* values: angle */
  TMATH_OP_ROTATE_AT = 7,        /* values: angle, px, py */
  TMATH_OP_RESET = 8,
  TMATH_OP_DRAW_CHAR = 9,        /* ints: UTF-16 unit; values: x, y */
  TMATH_OP_DRAW_TEXT = 10,       /* text; values: x, y */
  TMATH_OP_DRAW_LINE = 11,       /* values: x1, y1, x2, y2 */
  TMATH_OP_DRAW_RECT = 12,       /* values: x, y, w, h */
  TMATH_OP_FILL_RECT = 13,       /* values: x, y, w, h */
  TMATH_OP_DRAW_ROUND_RECT = 14, /* values: x, y, w, h, rx, ry */
  TMATH_OP_FILL_ROUND_RECT = 15, /* values: x, y, w, h, rx, ry */
  TMATH_OP_LAYOUT = 16,          /* a text run: family, style, pixel size, text; values: x, y */
};

/* Text and family point into the formula and live as long as it does. */
typedef struct TMathOp {
  int32_t kind;
  uint32_t color;
  int32_t ints[2];
  float values[6];
  const uint16_t *text;
  size_t text_len;
  const uint8_t *family;
  size_t family_len;
  int32_t style;
  int32_t pixel_size;
} TMathOp;

size_t tmath_op_count(const TMathFormula *formula);

/* Fills op with the op at index; returns 0 when there is none. */
int32_t tmath_op(const TMathFormula *formula, size_t index, TMathOp *op);

#ifdef __cplusplus
}
#endif

#endif /* TMATH_H */
