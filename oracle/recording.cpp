#include "recording.h"

#include "common.h"

#include <cstdio>

namespace oracle {
namespace {

Recorder *CurrentRecorder = nullptr;

void Measured(std::string line) {
	if (CurrentRecorder) {
		CurrentRecorder->measures.push_back(std::move(line));
	}
}

bool Recording() {
	return CurrentRecorder != nullptr;
}

void Op(std::string line) {
	if (CurrentRecorder) {
		CurrentRecorder->ops.push_back(std::move(line));
	}
}

// qRound(float) as Font_qt applies it before QFont::setPixelSize.
int RoundPixelSize(float size) {
	const auto rounded = (size >= 0.f) ? int(size + 0.5f) : int(size - 0.5f);
	return (rounded > 1) ? rounded : 1;
}

// QString construction in graphic_qt.cpp stops at the first NUL.
std::wstring TruncatedAtNul(const std::wstring &text) {
	const auto nul = text.find(L'\0');
	return (nul == std::wstring::npos) ? text : text.substr(0, nul);
}

} // namespace

void SetRecorder(Recorder *recorder) {
	CurrentRecorder = recorder;
}

std::string FormatFloat(float value) {
	char buffer[64];
	std::snprintf(buffer, sizeof(buffer), "%.9g", double(value));
	return buffer;
}

std::string FormatText(const std::wstring &text) {
	auto result = std::string("[");
	char buffer[16];
	for (size_t i = 0; i != text.size(); ++i) {
		std::snprintf(
			buffer,
			sizeof(buffer),
			(i ? " %x" : "%x"),
			unsigned(text[i]) & 0xFFFFu);
		result += buffer;
	}
	return result + "]";
}

tex::Rect MeasureText(
		const std::string &family,
		int style,
		float size,
		const std::wstring &text) {
	auto width = 0.f;
	for (const auto unit : text) {
		const auto code = unsigned(unit) & 0xFFFFu;
		width += (0.5f + float(code % 13u) * 0.03125f) * size;
	}
	if (style & tex::BOLD) {
		width *= 1.0625f;
	}
	if (style & tex::ITALIC) {
		width += 0.125f * size;
	}
	const auto familyShift = float(family.size() % 5u) * 0.015625f * size;
	return tex::Rect(0.0625f * size + familyShift, -0.75f * size, width, size);
}

} // namespace oracle

namespace tex {
namespace {

using oracle::FormatFloat;
using oracle::FormatText;
using oracle::Op;

class Font_rec : public Font {
public:
	Font_rec(std::string family, int style, float size, bool file)
	: _family(std::move(family))
	, _style(style)
	, _pixelSize(oracle::RoundPixelSize(size))
	, _file(file) {
	}

	float getSize() const override {
		return float(_pixelSize);
	}

	sptr<Font> deriveFont(int style) const override {
		return sptrOf<Font_rec>(_family, style, getSize(), false);
	}

	bool operator==(const Font &f) const override {
		const auto &o = static_cast<const Font_rec&>(f);
		return (_family == o._family)
			&& (_pixelSize == o._pixelSize)
			&& (_style == o._style);
	}

	bool operator!=(const Font &f) const override {
		return !(*this == f);
	}

	const std::string &family() const {
		return _family;
	}

	int style() const {
		return _style;
	}

	std::string describe() const {
		return (_file ? "file:" : "name:")
			+ _family
			+ "/" + std::to_string(_style)
			+ "@" + std::to_string(_pixelSize);
	}

private:
	std::string _family;
	int _style = PLAIN;
	int _pixelSize = 1;
	bool _file = false;

};

class TextLayout_rec : public TextLayout {
public:
	TextLayout_rec(const std::wstring &src, const sptr<Font_rec> &font)
	: _font(font)
	, _text(oracle::TruncatedAtNul(src)) {
	}

	void getBounds(Rect &bounds) override {
		bounds = oracle::MeasureText(
			_font->family(),
			_font->style(),
			_font->getSize(),
			_text);
		// Nothing records under --bench, so the line is not built there.
		if (!oracle::Recording()) {
			return;
		}
		oracle::Measured("measure " + _font->describe()
			+ " " + FormatText(_text)
			+ " -> " + FormatFloat(bounds.x)
			+ " " + FormatFloat(bounds.y)
			+ " " + FormatFloat(bounds.w)
			+ " " + FormatFloat(bounds.h));
	}

	void draw(Graphics2D &g2, float x, float y) override {
		Op("layout " + _font->describe()
			+ " " + FormatText(_text)
			+ " " + FormatFloat(x)
			+ " " + FormatFloat(y));
	}

private:
	sptr<Font_rec> _font;
	std::wstring _text;

};

} // namespace

Font *Font::create(const std::string &file, float size) {
	return new Font_rec(file, PLAIN, size, true);
}

sptr<Font> Font::_create(const std::string &name, int style, float size) {
	return sptrOf<Font_rec>(name, style, size, false);
}

sptr<TextLayout> TextLayout::create(
		const std::wstring &src,
		const sptr<Font> &font) {
	return sptrOf<TextLayout_rec>(src, std::static_pointer_cast<Font_rec>(font));
}

} // namespace tex

namespace oracle {

// Mirrors Graphics2D_qt: the same initial state, the same state the getters
// report, and the same effect of scale()/reset() on sx()/sy().
class Graphics2D_rec : public tex::Graphics2D {
public:
	Graphics2D_rec() {
		_color = tex::black;
		_stroke = tex::Stroke();
		_font = &_defaultFont;
	}

	void setColor(tex::color c) override {
		_color = c;
		char buffer[16];
		std::snprintf(buffer, sizeof(buffer), "%08x", unsigned(c));
		Op(std::string("setColor ") + buffer);
	}

	tex::color getColor() const override {
		return _color;
	}

	void setStroke(const tex::Stroke &s) override {
		_stroke = s;
		Op("setStroke " + FormatFloat(s.lineWidth)
			+ " " + FormatFloat(s.miterLimit)
			+ " " + std::to_string(int(s.cap))
			+ " " + std::to_string(int(s.join)));
	}

	const tex::Stroke &getStroke() const override {
		return _stroke;
	}

	void setStrokeWidth(float w) override {
		_stroke.lineWidth = w;
		Op("setStrokeWidth " + FormatFloat(w));
	}

	const tex::Font *getFont() const override {
		return _font;
	}

	void setFont(const tex::Font *font) override {
		_font = font;
		Op("setFont " + static_cast<const tex::Font_rec*>(font)->describe());
	}

	void translate(float dx, float dy) override {
		Op("translate " + FormatFloat(dx) + " " + FormatFloat(dy));
	}

	void scale(float sx, float sy) override {
		_sx *= sx;
		_sy *= sy;
		Op("scale " + FormatFloat(sx) + " " + FormatFloat(sy));
	}

	void rotate(float angle) override {
		Op("rotate " + FormatFloat(angle));
	}

	void rotate(float angle, float px, float py) override {
		Op("rotateAt " + FormatFloat(angle)
			+ " " + FormatFloat(px)
			+ " " + FormatFloat(py));
	}

	void reset() override {
		_sx = _sy = 1.f;
		Op("reset");
	}

	float sx() const override {
		return _sx;
	}

	float sy() const override {
		return _sy;
	}

	void drawChar(wchar_t c, float x, float y) override {
		Op("drawChar " + FormatText(std::wstring(1, c))
			+ " " + FormatFloat(x)
			+ " " + FormatFloat(y));
	}

	void drawText(const std::wstring &t, float x, float y) override {
		Op("drawText " + FormatText(t)
			+ " " + FormatFloat(x)
			+ " " + FormatFloat(y));
	}

	void drawLine(float x1, float y1, float x2, float y2) override {
		Op("drawLine " + FormatFloat(x1) + " " + FormatFloat(y1)
			+ " " + FormatFloat(x2) + " " + FormatFloat(y2));
	}

	void drawRect(float x, float y, float w, float h) override {
		Op("drawRect " + FormatFloat(x) + " " + FormatFloat(y)
			+ " " + FormatFloat(w) + " " + FormatFloat(h));
	}

	void fillRect(float x, float y, float w, float h) override {
		Op("fillRect " + FormatFloat(x) + " " + FormatFloat(y)
			+ " " + FormatFloat(w) + " " + FormatFloat(h));
	}

	void drawRoundRect(
			float x,
			float y,
			float w,
			float h,
			float rx,
			float ry) override {
		Op("drawRoundRect " + FormatFloat(x) + " " + FormatFloat(y)
			+ " " + FormatFloat(w) + " " + FormatFloat(h)
			+ " " + FormatFloat(rx) + " " + FormatFloat(ry));
	}

	void fillRoundRect(
			float x,
			float y,
			float w,
			float h,
			float rx,
			float ry) override {
		Op("fillRoundRect " + FormatFloat(x) + " " + FormatFloat(y)
			+ " " + FormatFloat(w) + " " + FormatFloat(h)
			+ " " + FormatFloat(rx) + " " + FormatFloat(ry));
	}

private:
	tex::Font_rec _defaultFont = tex::Font_rec(
		"SansSerif",
		tex::PLAIN,
		20.f,
		false);
	tex::color _color = 0;
	tex::Stroke _stroke;
	const tex::Font *_font = nullptr;
	float _sx = 1.f;
	float _sy = 1.f;

};

std::unique_ptr<tex::Graphics2D> MakeRecordingGraphics() {
	return std::make_unique<Graphics2D_rec>();
}

} // namespace oracle
