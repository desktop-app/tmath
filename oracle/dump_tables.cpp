// --dump-tables: every static table MicroTeX holds after initBundled(), in a
// canonical text form. tools/gen_tables.py generates the Rust tables from this
// output, and the engine's table test re-dumps them and compares, so the two
// can only agree.
//
// Reaches private members through the `friend struct OracleAccess;` lines on
// the tmath-oracle branch of desktop-app/MicroTeX.
#include "dump_tables.h"

#include "recording.h"

#include "atom/atom_basic.h"
#include "atom/atom_char.h"
#include "atom/atom_space.h"
#include "core/formula.h"
#include "core/glue.h"
#include "core/macro.h"
#include "core/parser.h"
#include "fonts/alphabet.h"
#include "fonts/font_info.h"
#include "fonts/fonts.h"

#include <cstdio>

namespace tex {

struct OracleAccess {
	static void Dump(std::ostream &out);
};

namespace {

using oracle::FormatFloat;
using oracle::FormatText;

std::string Narrow(const std::string &value) {
	auto result = std::wstring();
	for (const auto c : value) {
		result.push_back(wchar_t(uint8_t(c)));
	}
	return FormatText(result);
}

template <typename Array>
void DumpRows(
		std::ostream &out,
		const char *tag,
		int fontId,
		const Array &array,
		size_t columns,
		bool floats) {
	for (size_t row = 0; row != array.rows(); ++row) {
		const auto item = array[row];
		out << tag << " " << fontId;
		for (size_t column = 0; column != columns; ++column) {
			if (floats && column >= ((columns == 5) ? 1 : 2)) {
				out << " " << FormatFloat(float(item[column]));
			} else {
				out << " " << (long long)(item[column]);
			}
		}
		out << "\n";
	}
}

std::string CharFontText(const CharFont *font) {
	if (!font) {
		return "null";
	}
	return std::to_string(unsigned(font->chr) & 0xFFFFu)
		+ " " + std::to_string(font->fontId)
		+ " " + std::to_string(font->boldFontId);
}

} // namespace

void OracleAccess::Dump(std::ostream &out) {
	const auto &infos = FontInfo::_infos;
	for (size_t id = 0; id != FontInfo::_names.size(); ++id) {
		out << "fontname " << id << " " << FontInfo::_names[id] << "\n";
	}
	for (size_t id = 0; id != infos.size(); ++id) {
		const auto info = infos[id];
		if (!info) {
			out << "font " << id << " null\n";
			continue;
		}
		out << "font " << id
			<< " " << info->_path
			<< " " << FormatFloat(info->_xHeight)
			<< " " << FormatFloat(info->_space)
			<< " " << FormatFloat(info->_quad)
			<< " " << (long long)(info->_skewChar)
			<< " " << info->_boldId
			<< " " << info->_romanId
			<< " " << info->_ssId
			<< " " << info->_ttId
			<< " " << info->_itId
			<< "\n";
		DumpRows(out, "metric", int(id), info->_metrics, 5, true);
		DumpRows(out, "ext", int(id), info->_extensions, 5, false);
		DumpRows(out, "larger", int(id), info->_nextLargers, 3, false);
		DumpRows(out, "lig", int(id), info->_lig, 3, false);
		DumpRows(out, "kern", int(id), info->_kern, 3, true);
	}

	for (auto i = 0; i != 4; ++i) {
		out << "textstyle-default " << i << " "
			<< DefaultTeXFont::_defaultTextStyleMappings[i] << "\n";
	}
	for (const auto &[name, fonts] : DefaultTeXFont::_textStyleMappings) {
		for (size_t i = 0; i != fonts.size(); ++i) {
			out << "textstyle " << name << " " << i << " "
				<< CharFontText(fonts[i]) << "\n";
		}
	}
	for (const auto &[name, font] : DefaultTeXFont::_symbolMappings) {
		out << "symfont " << name << " " << CharFontText(font) << "\n";
	}
	for (const auto &[name, value] : DefaultTeXFont::_parameters) {
		out << "param " << name << " " << FormatFloat(value) << "\n";
	}
	for (const auto &[name, value] : DefaultTeXFont::_generalSettings) {
		out << "general " << name << " " << FormatFloat(value) << "\n";
	}

	for (const auto &[name, symbol] : SymbolAtom::_symbols) {
		out << "symbol " << name
			<< " " << int(symbol->_type)
			<< " " << int(symbol->_limitsType)
			<< " " << (unsigned(symbol->_unicode) & 0xFFFFu)
			<< "\n";
	}

	for (const auto &[name, code] : Formula::_predefinedTeXFormulasAsString) {
		out << "formula " << FormatText(name) << " " << FormatText(code) << "\n";
	}
	for (const auto &[code, value] : Formula::_symbolMappings) {
		out << "fsym " << code << " " << Narrow(value) << "\n";
	}
	for (const auto &[code, value] : Formula::_symbolTextMappings) {
		out << "fsymtext " << code << " " << Narrow(value) << "\n";
	}
	for (const auto &[code, value] : Formula::_symbolFormulaMappings) {
		out << "fsymformula " << code << " " << Narrow(value) << "\n";
	}

	for (const auto &[name, info] : MacroInfo::_commands) {
		const auto kind = dynamic_cast<const PreDefMacro*>(info)
			? "predef"
			: dynamic_cast<const InflationMacroInfo*>(info)
			? "inflation"
			: "other";
		out << "macro " << FormatText(name)
			<< " " << info->_argc
			<< " " << info->_posOpts
			<< " " << kind
			<< "\n";
	}
	for (const auto &[name, code] : NewCommandMacro::_codes) {
		out << "newcmd-code " << FormatText(name) << " " << FormatText(code) << "\n";
	}
	for (const auto &[name, code] : NewCommandMacro::_replacements) {
		out << "newcmd-repl " << FormatText(name) << " " << FormatText(code) << "\n";
	}

	for (const auto &[name, value] : ColorAtom::_colors) {
		char buffer[16];
		std::snprintf(buffer, sizeof(buffer), "%08x", unsigned(value));
		out << "color " << name << " " << buffer << "\n";
	}
	for (auto i = 0; i != SpaceAtom::_unitsCount; ++i) {
		out << "unit " << SpaceAtom::_units[i].first
			<< " " << int(SpaceAtom::_units[i].second) << "\n";
	}

	for (auto i = 0; i != Glue::GLUE_TYPE_COUNT; ++i) {
		const auto &glue = Glue::_glueTypes[i];
		out << "glue " << i
			<< " " << glue._space
			<< " " << glue._stretch
			<< " " << glue._shrink << "\n";
	}
	for (auto l = 0; l != Glue::TYPE_COUNT; ++l) {
		for (auto r = 0; r != Glue::TYPE_COUNT; ++r) {
			out << "gluetable " << l << " " << r;
			for (auto s = 0; s != Glue::STYLE_COUNT; ++s) {
				out << " " << int(Glue::_table[l][r][s]);
			}
			out << "\n";
		}
	}

	for (const auto &[from, to] : TeXParser::SUP_SCRIPT_MAP) {
		out << "supmap " << (unsigned(from) & 0xFFFFu) << " " << int(to) << "\n";
	}
	for (const auto &[from, to] : TeXParser::SUB_SCRIPT_MAP) {
		out << "submap " << (unsigned(from) & 0xFFFFu) << " " << int(to) << "\n";
	}
	for (const auto &content : TeXParser::_unparsedContents) {
		out << "unparsed " << FormatText(content) << "\n";
	}
	for (const auto block : UnicodeBlock::_defined) {
		out << "block " << (unsigned(block->_start) & 0xFFFFu)
			<< " " << (unsigned(block->_end) & 0xFFFFu) << "\n";
	}
}

} // namespace tex

namespace oracle {

void DumpTables(std::ostream &out) {
	tex::OracleAccess::Dump(out);
}

} // namespace oracle
