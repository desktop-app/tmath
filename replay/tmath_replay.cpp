#include "tmath_replay.h"

#include "fonts/font_info.h"
#include "graphic/graphic.h"

#include <string>
#include <vector>

namespace tmath {
namespace {

[[nodiscard]] std::wstring Wide(const uint16_t *text, size_t length) {
	auto result = std::wstring();
	result.reserve(length);
	for (auto i = size_t(0); i != length; ++i) {
		result.push_back(wchar_t(text[i]));
	}
	return result;
}

[[nodiscard]] std::string Narrow(const uint8_t *text, size_t length) {
	return std::string(reinterpret_cast<const char*>(text), length);
}

[[nodiscard]] const tex::Font *TexFont(int32_t id) {
	const auto &infos = tex::FontInfo::__infos();
	if (id < 0 || size_t(id) >= infos.size() || !infos[id]) {
		return nullptr;
	}
	return tex::FontInfo::getFont(id);
}

[[nodiscard]] bool Valid(const TMathOp &op) {
	const auto text = (op.text != nullptr) || (op.text_len == 0);
	switch (op.kind) {
	case TMATH_OP_SET_COLOR:
	case TMATH_OP_SET_STROKE:
	case TMATH_OP_SET_STROKE_WIDTH:
	case TMATH_OP_TRANSLATE:
	case TMATH_OP_SCALE:
	case TMATH_OP_ROTATE:
	case TMATH_OP_ROTATE_AT:
	case TMATH_OP_RESET:
	case TMATH_OP_DRAW_CHAR:
	case TMATH_OP_DRAW_LINE:
	case TMATH_OP_DRAW_RECT:
	case TMATH_OP_FILL_RECT:
	case TMATH_OP_DRAW_ROUND_RECT:
	case TMATH_OP_FILL_ROUND_RECT:
		return true;
	case TMATH_OP_SET_FONT:
		return TexFont(op.ints[0]) != nullptr;
	case TMATH_OP_DRAW_TEXT:
		return text;
	case TMATH_OP_LAYOUT:
		return text && (op.family != nullptr || op.family_len == 0);
	}
	return false;
}

void Apply(const TMathOp &op, tex::Graphics2D &g2) {
	const auto v = op.values;
	switch (op.kind) {
	case TMATH_OP_SET_COLOR:
		g2.setColor(tex::color(op.color));
		break;
	case TMATH_OP_SET_STROKE:
		g2.setStroke(tex::Stroke(
			v[0],
			tex::Cap(op.ints[0]),
			tex::Join(op.ints[1]),
			v[1]));
		break;
	case TMATH_OP_SET_STROKE_WIDTH:
		g2.setStrokeWidth(v[0]);
		break;
	case TMATH_OP_SET_FONT:
		g2.setFont(TexFont(op.ints[0]));
		break;
	case TMATH_OP_TRANSLATE:
		g2.translate(v[0], v[1]);
		break;
	case TMATH_OP_SCALE:
		g2.scale(v[0], v[1]);
		break;
	case TMATH_OP_ROTATE:
		g2.rotate(v[0]);
		break;
	case TMATH_OP_ROTATE_AT:
		g2.rotate(v[0], v[1], v[2]);
		break;
	case TMATH_OP_RESET:
		g2.reset();
		break;
	case TMATH_OP_DRAW_CHAR:
		g2.drawChar(wchar_t(uint16_t(op.ints[0])), v[0], v[1]);
		break;
	case TMATH_OP_DRAW_TEXT:
		g2.drawText(Wide(op.text, op.text_len), v[0], v[1]);
		break;
	case TMATH_OP_DRAW_LINE:
		g2.drawLine(v[0], v[1], v[2], v[3]);
		break;
	case TMATH_OP_DRAW_RECT:
		g2.drawRect(v[0], v[1], v[2], v[3]);
		break;
	case TMATH_OP_FILL_RECT:
		g2.fillRect(v[0], v[1], v[2], v[3]);
		break;
	case TMATH_OP_DRAW_ROUND_RECT:
		g2.drawRoundRect(v[0], v[1], v[2], v[3], v[4], v[5]);
		break;
	case TMATH_OP_FILL_ROUND_RECT:
		g2.fillRoundRect(v[0], v[1], v[2], v[3], v[4], v[5]);
		break;
	case TMATH_OP_LAYOUT: {
		const auto font = tex::Font::_create(
			Narrow(op.family, op.family_len),
			op.style,
			float(op.pixel_size));
		tex::TextLayout::create(Wide(op.text, op.text_len), font)->draw(
			g2,
			v[0],
			v[1]);
	} break;
	}
}

} // namespace

TMathRect MeasureText(
		void *context,
		const uint8_t *family,
		size_t familyLength,
		int32_t style,
		int32_t pixelSize,
		const uint16_t *text,
		size_t textLength) {
	auto bounds = tex::Rect();
	try {
		const auto font = tex::Font::_create(
			Narrow(family, familyLength),
			style,
			float(pixelSize));
		tex::TextLayout::create(Wide(text, textLength), font)->getBounds(
			bounds);
	} catch (...) {
		return TMathRect();
	}
	return TMathRect{ bounds.x, bounds.y, bounds.w, bounds.h };
}

bool Replay(const TMathFormula *formula, tex::Graphics2D &g2) {
	const auto count = tmath_op_count(formula);
	auto ops = std::vector<TMathOp>(count);
	for (auto i = size_t(0); i != count; ++i) {
		if (!tmath_op(formula, i, &ops[i]) || !Valid(ops[i])) {
			return false;
		}
	}
	for (const auto &op : ops) {
		Apply(op, g2);
	}
	return true;
}

} // namespace tmath
