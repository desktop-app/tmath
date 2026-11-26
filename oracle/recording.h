// Recording platform backend for the C++ MicroTeX oracle.
//
// Stands in for platform/qt/graphic_qt.cpp: every Graphics2D call and every
// TextLayout measurement is appended to the current Recorder as one canonical
// text line, and the state Graphics2D getters return (color, stroke, font,
// sx, sy) is tracked exactly as Graphics2D_qt tracks it, because Box::draw
// code reads that state back and branches on it.
//
// TextLayout::getBounds uses MeasureText() below instead of QFontMetricsF.
// The Rust port implements the same function bit-for-bit, so the two sides can
// be compared without Qt. The real Qt measurement is exercised by the pixel
// tests, which feed the same answers to both sides.
#pragma once

#include "graphic/graphic.h"

#include <memory>
#include <string>
#include <vector>

namespace oracle {

struct Recorder {
	std::vector<std::string> measures;
	std::vector<std::string> ops;
};

void SetRecorder(Recorder *recorder);

[[nodiscard]] std::unique_ptr<tex::Graphics2D> MakeRecordingGraphics();

[[nodiscard]] std::string FormatFloat(float value);
[[nodiscard]] std::string FormatText(const std::wstring &text);

// Deterministic stand-in for QFontMetricsF(font).boundingRect(text).
// The family, style and pixel size are what Font_qt would have been built
// with; text is a sequence of UTF-16 code units, as tdesktop passes them.
[[nodiscard]] tex::Rect MeasureText(
	const std::string &family,
	int style,
	float size,
	const std::wstring &text);

} // namespace oracle
