// --env-queries: the style environment, unit conversion, glue and length
// parsing, answered for every style, unit, atom type pair and a large set of
// length strings. The engine's environment test asks the same questions and
// compares the answers line by line.
//
// The length strings are generated here and written into the output as UTF-16
// units, so the test reads its inputs from the golden file instead of
// regenerating them.
#include "dump_tables.h"

#include "recording.h"

#include "atom/atom_space.h"
#include "box/box_single.h"
#include "core/core.h"
#include "core/glue.h"
#include "fonts/fonts.h"

#include <cstdint>
#include <string>
#include <vector>

namespace oracle {
namespace {

using namespace tex;

constexpr float kSizes[] = { 1024.f, 20.f, 13.5f, 0.75f };

const char *kDerivations[] = { "copy", "copytf", "cramp", "dnom", "num", "root", "sub", "sup" };

sptr<Environment> &Derive(Environment &env, int which) {
	switch (which) {
	case 0: return env.copy();
	case 1: return env.copy(env.getTeXFont()->copy());
	case 2: return env.crampStyle();
	case 3: return env.dnomStyle();
	case 4: return env.numStyle();
	case 5: return env.rootStyle();
	case 6: return env.subStyle();
	default: return env.supStyle();
	}
}

std::string Describe(const Environment &env) {
	return std::to_string(int(env.getStyle()))
		+ " " + std::to_string(env.getLastFontId())
		+ " " + FormatFloat(env.getTextWidth())
		+ " " + FormatFloat(env.getInterline())
		+ " " + FormatFloat(env.getSpace())
		+ " " + FormatFloat(env.getSize())
		+ " " + std::to_string(int(env.getSmallCap()))
		+ " " + (env.getTextStyle().empty() ? std::string("-") : env.getTextStyle())
		+ " " + FormatFloat(env.getScaleFactor());
}

std::string BoxText(const sptr<Box> &box) {
	const auto glue = dynamic_cast<GlueBox*>(box.get());
	if (!glue) {
		return "not-glue";
	}
	return FormatFloat(glue->_width)
		+ " " + FormatFloat(glue->_stretch)
		+ " " + FormatFloat(glue->_shrink);
}

void EnvironmentQueries(std::ostream &out, float size, int style) {
	const auto tag = FormatFloat(size) + " " + std::to_string(style);
	const auto font = sptr<TeXFont>(new DefaultTeXFont(size));
	auto env = Environment(TexStyle(style), font);
	out << "env " << tag << " -> " << Describe(env) << "\n";
	for (auto unit = -2; unit != 16; ++unit) {
		out << "factor " << tag << " " << unit << " -> "
			<< FormatFloat(SpaceAtom::getFactor(UnitType(unit), env)) << "\n";
	}
	for (auto which = 0; which != 8; ++which) {
		auto &derived = *Derive(env, which);
		out << "derive " << tag << " " << kDerivations[which] << " -> "
			<< Describe(derived) << "\n";
		auto line = std::string();
		for (auto next = 0; next != 8; ++next) {
			line += " " + std::to_string(int(Derive(derived, next)->getStyle()));
		}
		out << "chain " << tag << " " << kDerivations[which] << " ->" << line << "\n";
	}

	auto modified = Environment(TexStyle(style), font);
	modified.setTextStyle("mathbb");
	modified.setSmallCap(true);
	modified.setScaleFactor(2.5f);
	modified.setLastFontId(3);
	modified.setTextWidth(UnitType::em, 7.f);
	modified.setInterline(UnitType::mu, 3.f);
	out << "modified " << tag << " -> " << Describe(modified) << "\n";
	for (auto which = 0; which != 8; ++which) {
		out << "mderive " << tag << " " << kDerivations[which] << " -> "
			<< Describe(*Derive(modified, which)) << "\n";
	}

	for (auto id = -1; id != int(FontInfo::__infos().size()); ++id) {
		auto withFont = Environment(TexStyle(style), font);
		withFont.setLastFontId(id);
		out << "lastfont " << tag << " " << id << " -> "
			<< withFont.getLastFontId()
			<< " " << FormatFloat(SpaceAtom::getFactor(UnitType::ex, withFont))
			<< " " << FormatFloat(withFont.getInterline()) << "\n";
	}

	for (auto unit = -1; unit != 15; ++unit) {
		auto sized = Environment(TexStyle(style), font, UnitType(unit), 37.5f);
		out << "width " << tag << " " << unit << " -> " << Describe(sized) << "\n";
		sized.setInterline(UnitType(unit), 2.25f);
		out << "interline " << tag << " " << unit << " -> "
			<< FormatFloat(sized.getInterline()) << "\n";
	}

	for (auto l = -1; l != 15; ++l) {
		for (auto r = -1; r != 15; ++r) {
			out << "glue " << tag << " " << l << " " << r << " -> "
				<< FormatFloat(Glue::getSpace(AtomType(l), AtomType(r), env))
				<< " " << BoxText(Glue::get(AtomType(l), AtomType(r), env)) << "\n";
		}
	}
	for (auto type = -3; type != 4; ++type) {
		out << "skip " << tag << " " << type << " -> "
			<< FormatFloat(Glue::getSpace(SpaceType(type), env))
			<< " " << BoxText(Glue::get(SpaceType(type), env)) << "\n";
	}
}

// A small deterministic generator, so the inputs are the same on every build.
struct Generator {
	uint32_t state = 0x2545F491u;

	uint32_t next() {
		state ^= state << 13;
		state ^= state >> 17;
		state ^= state << 5;
		return state;
	}

	int below(int bound) {
		return int(next() % uint32_t(bound));
	}
};

std::vector<std::wstring> LengthInputs() {
	auto result = std::vector<std::wstring>();
	const auto add = [&](const std::wstring &value) {
		result.push_back(value);
	};
	const auto units = std::vector<std::wstring>{
		L"", L"bp", L"cc", L"cm", L"dd", L"em", L"ex", L"in", L"mm", L"mu",
		L"pc", L"pica", L"pix", L"pixel", L"pt", L"px", L"sp", L"x8",
		L"PT", L"Em", L"pT", L"ptx", L"p", L"zz", L" pt", L"pt ", L"pt\t",
	};
	const auto numbers = std::vector<std::wstring>{
		L"", L"1", L"-1", L"+1", L"0", L"-0", L"+0", L"1.5", L"-2.25", L".5",
		L"5.", L".", L"-", L"+", L"--1", L"+-1", L"-+1", L"1.2.3", L"1,5",
		L" 4", L"\t5", L"\n6", L"\v7", L"\f8", L"\r9", L" \t \r\n 10",
		L"- 1", L"+ 1", L"1 2", L"00012", L"0000.0001", L"-.0", L"0.",
		L"\x1c" L"1", L"\x7f" L"1", L"340282346638528859811704183484516925440",
		L"340282356779733661637539395458142568448",
		L"340282366920938463463374607431768211456",
		L"16777217", L"16777219", L"0.1", L"0.2", L"0.3", L"3.14159265358979",
		L"1.00000005960464477539062500",
		L"1.000000059604644775390625001",
		L"0.99999997019767761230468750",
		L"0.0000000000000000000000000000000000000000000014",
		L"0.0000000000000000000000000000000000000000000007",
		L"0.00000000000000000000000000000000000000000000070000001",
		L"0.000000000000000000000000000000000000011754942",
		L"123456789012345678901234567890",
	};
	for (const auto &number : numbers) {
		for (const auto &unit : units) {
			add(number + unit);
		}
	}
	add(L"1" + std::wstring(40, L'0') + L"pt");
	add(L"0." + std::wstring(60, L'0') + L"1pt");
	add(L"1.00000005960464477539062500" + std::wstring(900, L'0') + L"1pt");
	add(L"1.00000005960464477539062500" + std::wstring(900, L'0') + L"pt");
	add(std::wstring(800, L'9') + L"." + std::wstring(10, L'9'));
	add(std::wstring(1000, L'0') + L"1.5em");
	add(L"0." + std::wstring(1200, L'0') + L"1");
	add(L"1" + std::wstring(37, L'0') + L"mu");
	add(L"3" + std::wstring(38, L'0'));
	add(L"34" + std::wstring(37, L'0'));
	add(L"35" + std::wstring(37, L'0'));
	const auto special = std::vector<std::wstring>{
		L"1\xDC70\xDC74", L"1\xD800pt", L"1\xD800", L"\xD83D\xDE00" L"1pt",
		L"1\xDC00pt", L"1\xDC00", L"1pt\xDC00xx", L"1\xD800\xDC00pt",
		L"\xFF11pt", L"1\xA0pt", L"1\x2009pt", L"\xDC31\xDC2E\xDC35\xDC65\xDC6D",
		L"2\xD800\xD800\xDC70t", L"1p\xDC74", L"1\x130", L"1\x131",
		L"1P\xDC54", std::wstring(L"1\0pt", 4), std::wstring(L"\0" L"1pt", 4),
		std::wstring(L"1pt\0x", 5), L"1\xDC20pt", L"\xDC20" L"1pt",
	};
	for (const auto &value : special) {
		add(value);
	}

	auto generator = Generator();
	const auto alphabet = std::wstring(L"0123456789.-+ \tptemxPTcinusdbaz8");
	const auto rare = std::vector<wchar_t>{
		wchar_t(0), wchar_t(0xDC70), wchar_t(0xDC74), wchar_t(0xD800),
		wchar_t(0xA0), wchar_t(0x3B1), wchar_t(0xDC2E), wchar_t(0xDC31),
	};
	for (auto i = 0; i != 6000; ++i) {
		auto value = std::wstring();
		const auto length = generator.below(13);
		for (auto j = 0; j != length; ++j) {
			if (generator.below(16) == 0) {
				value.push_back(rare[generator.below(int(rare.size()))]);
			} else {
				value.push_back(alphabet[generator.below(int(alphabet.size()))]);
			}
		}
		add(value);
	}
	for (auto i = 0; i != 4000; ++i) {
		auto value = std::wstring();
		if (generator.below(4) == 0) {
			value.push_back(L'-');
		}
		const auto digits = 1 + generator.below(generator.below(8) == 0 ? 120 : 24);
		const auto point = generator.below(digits + 1);
		const auto zeros = generator.below(6) == 0 ? generator.below(50) : 0;
		for (auto j = 0; j != digits; ++j) {
			if (j == point) {
				value.push_back(L'.');
				value.append(zeros, L'0');
			}
			value.push_back(wchar_t(L'0' + generator.below(10)));
		}
		value += units[generator.below(int(units.size()))];
		add(value);
	}
	return result;
}

} // namespace

void EnvQueries(std::ostream &out) {
	for (const auto size : kSizes) {
		for (auto style = 0; style != 8; ++style) {
			EnvironmentQueries(out, size, style);
		}
	}
	for (const auto &input : LengthInputs()) {
		const auto [unit, value] = SpaceAtom::getLength(input);
		out << "length " << FormatText(input) << " -> " << int(unit)
			<< " " << FormatFloat(value) << "\n";
	}
}

} // namespace oracle
