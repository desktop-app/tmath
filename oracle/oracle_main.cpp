// microtex_oracle: runs the C++ MicroTeX over a JSONL corpus and prints, per
// formula, everything the Rust port must reproduce: the parse status, the
// TeXRender metrics, the box tree, the text measurements requested during
// layout and the Graphics2D call stream of TeXRender::draw(g2, 0, 0).
//
// Input, one JSON object per line:
//   {"id": "...", "tex": "...", "size": 1024, "width": 38400,
//    "line": 256, "fg": 4294967295}
// Every field but "tex" is optional; the defaults are what tdesktop passes for
// a 16px formula (kFormulaExactMetricScale = 64). "tex" is passed to
// LaTeX::parse() as is: tdesktop's own rewriting (the \displaystyle prefix,
// full-width environments) belongs to the corpus generator.
//
// Output: "@ <id>" is printed and flushed before a formula is parsed, so a
// driver can tell which input crashed or hung the process.
//
// Modes:
//   microtex_oracle [--digest] [--no-boxes] < corpus.jsonl
//   microtex_oracle --bench <repeat> < corpus.jsonl
//   microtex_oracle --dump-tables
//   microtex_oracle --font-queries
//   microtex_oracle --env-queries
//   microtex_oracle --predefined-cache
//   microtex_oracle --replay [--digest] [--no-boxes] < corpus.jsonl
//     (built with TMATH_C_LIB: the Rust engine through tmath.h, painted by
//     tmath::Replay; no box tree)
#include "dump_tables.h"
#include "recording.h"

#ifdef TMATH_REPLAY
#include "tmath_replay.h"
#endif // TMATH_REPLAY

#include "latex.h"
#include "box/box.h"

#include <chrono>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <iostream>
#include <optional>
#include <string>
#include <typeinfo>
#include <vector>

#ifdef _WIN32
#include <windows.h>
#include <crtdbg.h>
#include <fcntl.h>
#include <io.h>
#elif defined(__GNUC__)
#include <cxxabi.h>
#endif

namespace {

struct Record {
	std::string id;
	std::string tex;
	float size = 1024.f;
	int width = 600 * 64;
	float line = 256.f;
	uint32_t fg = 0xFFFFFFFFu;
};

void SkipSpaces(const std::string &s, size_t &i) {
	while (i < s.size() && (s[i] == ' ' || s[i] == '\t' || s[i] == '\r')) {
		++i;
	}
}

void AppendUtf8(std::string &out, uint32_t code) {
	if (code < 0x80) {
		out += char(code);
	} else if (code < 0x800) {
		out += char(0xC0 | (code >> 6));
		out += char(0x80 | (code & 0x3F));
	} else if (code < 0x10000) {
		out += char(0xE0 | (code >> 12));
		out += char(0x80 | ((code >> 6) & 0x3F));
		out += char(0x80 | (code & 0x3F));
	} else {
		out += char(0xF0 | (code >> 18));
		out += char(0x80 | ((code >> 12) & 0x3F));
		out += char(0x80 | ((code >> 6) & 0x3F));
		out += char(0x80 | (code & 0x3F));
	}
}

std::optional<uint32_t> ReadHex4(const std::string &s, size_t &i) {
	if (i + 4 > s.size()) {
		return std::nullopt;
	}
	auto value = uint32_t(0);
	for (auto k = 0; k != 4; ++k) {
		const auto c = s[i++];
		value <<= 4;
		if (c >= '0' && c <= '9') {
			value |= uint32_t(c - '0');
		} else if (c >= 'a' && c <= 'f') {
			value |= uint32_t(c - 'a' + 10);
		} else if (c >= 'A' && c <= 'F') {
			value |= uint32_t(c - 'A' + 10);
		} else {
			return std::nullopt;
		}
	}
	return value;
}

std::optional<std::string> ReadString(const std::string &s, size_t &i) {
	if (i >= s.size() || s[i] != '"') {
		return std::nullopt;
	}
	++i;
	auto result = std::string();
	while (i < s.size()) {
		const auto c = s[i++];
		if (c == '"') {
			return result;
		} else if (c != '\\') {
			result += c;
			continue;
		} else if (i >= s.size()) {
			return std::nullopt;
		}
		const auto e = s[i++];
		switch (e) {
		case '"': result += '"'; break;
		case '\\': result += '\\'; break;
		case '/': result += '/'; break;
		case 'b': result += '\b'; break;
		case 'f': result += '\f'; break;
		case 'n': result += '\n'; break;
		case 'r': result += '\r'; break;
		case 't': result += '\t'; break;
		case 'u': {
			auto code = ReadHex4(s, i);
			if (!code) {
				return std::nullopt;
			}
			// A lone surrogate is kept as is (WTF-8), so inputs with broken
			// UTF-16 reach the library the way tdesktop would pass them.
			if (*code >= 0xD800 && *code < 0xDC00
				&& i + 1 < s.size() && s[i] == '\\' && s[i + 1] == 'u') {
				auto j = i + 2;
				const auto low = ReadHex4(s, j);
				if (low && *low >= 0xDC00 && *low < 0xE000) {
					code = 0x10000 + ((*code - 0xD800) << 10) + (*low - 0xDC00);
					i = j;
				}
			}
			AppendUtf8(result, *code);
		} break;
		default: return std::nullopt;
		}
	}
	return std::nullopt;
}

std::optional<double> ReadNumber(const std::string &s, size_t &i) {
	const auto start = i;
	while (i < s.size()
		&& (std::strchr("+-0123456789.eE", s[i]) != nullptr)) {
		++i;
	}
	if (i == start) {
		return std::nullopt;
	}
	return std::strtod(s.substr(start, i - start).c_str(), nullptr);
}

std::optional<Record> ParseRecord(const std::string &line) {
	auto result = Record();
	auto haveTex = false;
	auto i = size_t(0);
	SkipSpaces(line, i);
	if (i >= line.size() || line[i++] != '{') {
		return std::nullopt;
	}
	while (true) {
		SkipSpaces(line, i);
		if (i < line.size() && line[i] == '}') {
			break;
		}
		const auto key = ReadString(line, i);
		if (!key) {
			return std::nullopt;
		}
		SkipSpaces(line, i);
		if (i >= line.size() || line[i++] != ':') {
			return std::nullopt;
		}
		SkipSpaces(line, i);
		if (*key == "id" || *key == "tex") {
			const auto value = ReadString(line, i);
			if (!value) {
				return std::nullopt;
			}
			if (*key == "id") {
				result.id = *value;
			} else {
				result.tex = *value;
				haveTex = true;
			}
		} else {
			const auto value = ReadNumber(line, i);
			if (!value) {
				return std::nullopt;
			}
			if (*key == "size") {
				result.size = float(*value);
			} else if (*key == "width") {
				result.width = int(*value);
			} else if (*key == "line") {
				result.line = float(*value);
			} else if (*key == "fg") {
				result.fg = uint32_t(*value);
			}
		}
		SkipSpaces(line, i);
		if (i < line.size() && line[i] == ',') {
			++i;
		}
	}
	if (!haveTex) {
		return std::nullopt;
	}
	return result;
}

// tdesktop's ToWide(): one wchar_t per UTF-16 code unit, on every platform.
std::wstring ToUtf16Units(const std::string &utf8) {
	auto result = std::wstring();
	auto i = size_t(0);
	while (i < utf8.size()) {
		const auto c = uint8_t(utf8[i]);
		auto code = uint32_t(0xFFFD);
		auto length = size_t(1);
		if (c < 0x80) {
			code = c;
		} else if ((c >> 5) == 0x6 && i + 1 < utf8.size()) {
			code = ((c & 0x1F) << 6) | (uint8_t(utf8[i + 1]) & 0x3F);
			length = 2;
		} else if ((c >> 4) == 0xE && i + 2 < utf8.size()) {
			code = ((c & 0x0F) << 12)
				| ((uint8_t(utf8[i + 1]) & 0x3F) << 6)
				| (uint8_t(utf8[i + 2]) & 0x3F);
			length = 3;
		} else if ((c >> 3) == 0x1E && i + 3 < utf8.size()) {
			code = ((c & 0x07) << 18)
				| ((uint8_t(utf8[i + 1]) & 0x3F) << 12)
				| ((uint8_t(utf8[i + 2]) & 0x3F) << 6)
				| (uint8_t(utf8[i + 3]) & 0x3F);
			length = 4;
		}
		i += length;
		if (code >= 0x10000) {
			code -= 0x10000;
			result.push_back(wchar_t(0xD800 + (code >> 10)));
			result.push_back(wchar_t(0xDC00 + (code & 0x3FF)));
		} else {
			result.push_back(wchar_t(code));
		}
	}
	return result;
}

std::string BoxTypeName(const tex::Box &box) {
	auto name = std::string(typeid(box).name());
#if defined(__GNUC__) && !defined(_WIN32)
	auto status = 0;
	if (const auto demangled = abi::__cxa_demangle(
			name.c_str(),
			nullptr,
			nullptr,
			&status)) {
		name = demangled;
		std::free(demangled);
	}
#endif
	for (const auto prefix : { "class ", "struct ", "tex::" }) {
		const auto length = std::strlen(prefix);
		if (name.compare(0, length, prefix) == 0) {
			name = name.substr(length);
		}
	}
	return name;
}

void DumpBox(
		const tex::sptr<tex::Box> &box,
		int depth,
		std::vector<std::string> &out) {
	if (!box) {
		out.push_back("box " + std::string(depth * 2, ' ') + "null");
		return;
	}
	out.push_back("box " + std::string(depth * 2, ' ')
		+ BoxTypeName(*box)
		+ " " + oracle::FormatFloat(box->_width)
		+ " " + oracle::FormatFloat(box->_height)
		+ " " + oracle::FormatFloat(box->_depth)
		+ " " + oracle::FormatFloat(box->_shift)
		+ " " + std::to_string(int(box->_type)));
	for (const auto &child : box->descendants()) {
		DumpBox(child, depth + 1, out);
	}
}

struct Result {
	std::string status;
	std::string metrics;
	std::vector<std::string> boxes;
	oracle::Recorder recorder;
};

Result Run(const Record &record, bool boxes) {
	auto result = Result();
	oracle::SetRecorder(&result.recorder);
	auto render = std::unique_ptr<tex::TeXRender>();
	try {
		render.reset(tex::LaTeX::parse(
			ToUtf16Units(record.tex),
			record.width,
			record.size,
			record.line,
			record.fg));
		if (!render) {
			result.status = "error null-render";
		}
	} catch (const std::exception &e) {
		result.status = std::string("error ") + e.what();
	} catch (...) {
		result.status = "error unknown-exception";
	}
	if (render) {
		const auto insets = render->getInsets();
		result.metrics = "metrics "
			+ std::to_string(render->getWidth())
			+ " " + std::to_string(render->getHeight())
			+ " " + std::to_string(render->getDepth())
			+ " " + oracle::FormatFloat(render->getBaseline())
			+ " " + std::to_string(insets.left)
			+ " " + std::to_string(insets.top)
			+ " " + std::to_string(insets.right)
			+ " " + std::to_string(insets.bottom);
		if (boxes) {
			DumpBox(render->getBox(), 0, result.boxes);
		}
		try {
			auto graphics = oracle::MakeRecordingGraphics();
			render->draw(*graphics, 0, 0);
			result.status = "ok";
		} catch (const std::exception &e) {
			result.status = std::string("draw-error ") + e.what();
		} catch (...) {
			result.status = "draw-error unknown-exception";
		}
	}
	oracle::SetRecorder(nullptr);
	return result;
}

#ifdef TMATH_REPLAY
Result RunReplay(const Record &record) {
	auto result = Result();
	oracle::SetRecorder(&result.recorder);
	const auto wide = ToUtf16Units(record.tex);
	const auto units = std::vector<uint16_t>(wide.begin(), wide.end());
	const auto formula = tmath_layout(
		units.data(),
		units.size(),
		record.width,
		record.size,
		record.line,
		record.fg,
		tmath::MeasureText,
		nullptr);
	if (tmath_status(formula) != TMATH_OK) {
		result.status = std::string("error ")
			+ reinterpret_cast<const char*>(tmath_error(formula, nullptr));
	} else {
		int32_t insets[4] = { 0 };
		tmath_insets(formula, insets);
		result.metrics = "metrics "
			+ std::to_string(tmath_width(formula))
			+ " " + std::to_string(tmath_height(formula))
			+ " " + std::to_string(tmath_depth(formula))
			+ " " + oracle::FormatFloat(tmath_baseline(formula))
			+ " " + std::to_string(insets[0])
			+ " " + std::to_string(insets[1])
			+ " " + std::to_string(insets[2])
			+ " " + std::to_string(insets[3]);
		auto graphics = oracle::MakeRecordingGraphics();
		result.status = tmath::Replay(formula, *graphics)
			? "ok"
			: "draw-error invalid-display-list";
	}
	tmath_free(formula);
	oracle::SetRecorder(nullptr);
	return result;
}
#endif // TMATH_REPLAY

uint64_t Fnv1a(const std::vector<std::string> &lines, uint64_t hash) {
	for (const auto &line : lines) {
		for (const auto c : line) {
			hash ^= uint8_t(c);
			hash *= 0x100000001b3ull;
		}
		hash ^= uint8_t('\n');
		hash *= 0x100000001b3ull;
	}
	return hash;
}

void PrintFull(const Record &record, const Result &result) {
	std::cout << "status " << result.status << "\n";
	if (!result.metrics.empty()) {
		std::cout << result.metrics << "\n";
	}
	for (const auto &line : result.boxes) {
		std::cout << line << "\n";
	}
	for (const auto &line : result.recorder.measures) {
		std::cout << line << "\n";
	}
	for (const auto &line : result.recorder.ops) {
		std::cout << line << "\n";
	}
	std::cout << "end\n";
}

void PrintDigest(const Record &record, const Result &result) {
	auto hash = 0xcbf29ce484222325ull;
	hash = Fnv1a(result.boxes, hash);
	hash = Fnv1a(result.recorder.measures, hash);
	hash = Fnv1a(result.recorder.ops, hash);
	char buffer[32];
	std::snprintf(buffer, sizeof(buffer), "%016llx", (unsigned long long)hash);
	std::cout << "status " << result.status << "\n";
	if (!result.metrics.empty()) {
		std::cout << result.metrics << "\n";
	}
	std::cout << "digest " << buffer
		<< " ops=" << result.recorder.ops.size()
		<< " boxes=" << result.boxes.size()
		<< " measures=" << result.recorder.measures.size() << "\n";
	std::cout << "end\n";
}

int Bench(const std::vector<Record> &records, int repeat) {
	using Clock = std::chrono::steady_clock;
	for (const auto &record : records) {
		const auto wide = ToUtf16Units(record.tex);
		auto best = std::chrono::nanoseconds::max();
		auto failed = false;
		for (auto k = 0; k != repeat; ++k) {
			const auto start = Clock::now();
			try {
				delete tex::LaTeX::parse(
					wide,
					record.width,
					record.size,
					record.line,
					record.fg);
			} catch (...) {
				failed = true;
			}
			const auto elapsed = Clock::now() - start;
			if (elapsed < best) {
				best = std::chrono::duration_cast<std::chrono::nanoseconds>(
					elapsed);
			}
		}
		std::cout << record.id
			<< "\t" << (failed ? "error" : "ok")
			<< "\t" << best.count() << "\n";
	}
	return 0;
}

} // namespace

int main(int argc, char *argv[]) {
#ifdef _WIN32
	SetErrorMode(SEM_FAILCRITICALERRORS | SEM_NOGPFAULTERRORBOX);
	_set_abort_behavior(0, _WRITE_ABORT_MSG | _CALL_REPORTFAULT);
	_CrtSetReportMode(_CRT_ASSERT, _CRTDBG_MODE_FILE);
	_CrtSetReportFile(_CRT_ASSERT, _CRTDBG_FILE_STDERR);
	_setmode(_fileno(stdin), _O_BINARY);
	_setmode(_fileno(stdout), _O_BINARY);
#endif
	auto digest = false;
	auto boxes = true;
	auto bench = 0;
	auto tables = false;
	auto fontQueries = false;
	auto envQueries = false;
	auto predefinedCache = false;
	auto replay = false;
	for (auto i = 1; i < argc; ++i) {
		const auto arg = std::string(argv[i]);
		if (arg == "--digest") {
			digest = true;
		} else if (arg == "--no-boxes") {
			boxes = false;
		} else if (arg == "--font-queries") {
			fontQueries = true;
		} else if (arg == "--env-queries") {
			envQueries = true;
		} else if (arg == "--replay") {
			replay = true;
		} else if (arg == "--predefined-cache") {
			predefinedCache = true;
		} else if (arg == "--dump-tables") {
			tables = true;
		} else if (arg == "--bench" && i + 1 < argc) {
			bench = std::max(1, std::atoi(argv[++i]));
		} else {
			std::cerr << "Unknown argument: " << arg << "\n";
			return 2;
		}
	}
	tex::LaTeX::initBundled();
	if (fontQueries) {
		oracle::FontQueries(std::cout);
		return 0;
	}
	if (envQueries) {
		oracle::EnvQueries(std::cout);
		return 0;
	}
	if (tables) {
		oracle::DumpTables(std::cout);
		return 0;
	}
	if (predefinedCache) {
		oracle::PredefinedCache(std::cout);
		return 0;
	}

	auto records = std::vector<Record>();
	auto line = std::string();
	auto lineNumber = 0;
	while (std::getline(std::cin, line)) {
		++lineNumber;
		if (line.empty() || line == "\r") {
			continue;
		}
		auto record = ParseRecord(line);
		if (!record) {
			std::cerr << "Bad record at line " << lineNumber << "\n";
			return 2;
		}
		if (record->id.empty()) {
			record->id = std::to_string(lineNumber);
		}
		if (bench) {
			records.push_back(std::move(*record));
			continue;
		}
		std::cout << "@ " << record->id << "\n" << std::flush;
#ifdef TMATH_REPLAY
		const auto result = replay ? RunReplay(*record) : Run(*record, boxes);
#else // TMATH_REPLAY
		if (replay) {
			std::cerr << "Built without TMATH_C_LIB, --replay is unavailable.
";
			return 2;
		}
		const auto result = Run(*record, boxes);
#endif // TMATH_REPLAY
		if (digest) {
			PrintDigest(*record, result);
		} else {
			PrintFull(*record, result);
		}
		std::cout << std::flush;
	}
	return bench ? Bench(records, bench) : 0;
}
