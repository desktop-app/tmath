// Paints a tmath formula through MicroTeX's Graphics2D, and measures its text
// runs the way MicroTeX does. Compiled together with MicroTeX's platform and
// font sources, which stay in C++.
#pragma once

#include "tmath.h"

namespace tex {
class Graphics2D;
} // namespace tex

namespace tmath {

// The TMathMeasureText MicroTeX's own TextRenderingBox would answer with:
// TextLayout::getBounds() for Font::_create(family, style, pixelSize).
TMathRect MeasureText(
	void *context,
	const uint8_t *family,
	size_t familyLength,
	int32_t style,
	int32_t pixelSize,
	const uint16_t *text,
	size_t textLength);

// Makes the calls TeXRender::draw(g2, 0, 0) made for a formula tmath_layout()
// returned. Checks the whole list first and paints nothing if any op is
// malformed: an unknown kind, a font that is not loaded, missing text.
bool Replay(const TMathFormula *formula, tex::Graphics2D &g2);

} // namespace tmath
