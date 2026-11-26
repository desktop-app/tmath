# microtex_oracle: the C++ MicroTeX with a recording backend instead of Qt.
# It is the reference the Rust port is compared against. Build it for x64:
# 32-bit x86 returns floats through x87 and 64-bit macOS clang contracts
# a * b + c into FMA, both of which move results by an ulp.
cmake_minimum_required(VERSION 3.16)
project(microtex_oracle CXX)

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

set(MICROTEX_ROOT "${CMAKE_CURRENT_SOURCE_DIR}/MicroTeX" CACHE PATH
    "Checkout of desktop-app/MicroTeX the oracle is built from")
get_filename_component(microtex_src ${MICROTEX_ROOT}/src ABSOLUTE)
if (NOT EXISTS ${microtex_src}/render.h)
    message(FATAL_ERROR "MicroTeX sources not found in ${MICROTEX_ROOT}: run "
        "'git submodule update --init' or pass -DMICROTEX_ROOT=<checkout>.")
endif()

file(GLOB microtex_font_defs ${microtex_src}/res/font/*.def.cpp)
file(GLOB microtex_sym_defs ${microtex_src}/res/sym/*.def.cpp)

add_executable(microtex_oracle
    oracle_main.cpp
    dump_tables.cpp
    dump_tables.h
    env_queries.cpp
    recording.cpp
    recording.h
    ${microtex_src}/latex.cpp
    ${microtex_src}/render.cpp
    ${microtex_src}/atom/atom_basic.cpp
    ${microtex_src}/atom/atom_char.cpp
    ${microtex_src}/atom/atom_impl.cpp
    ${microtex_src}/atom/atom_matrix.cpp
    ${microtex_src}/atom/atom_row.cpp
    ${microtex_src}/atom/atom_space.cpp
    ${microtex_src}/atom/colors_def.cpp
    ${microtex_src}/atom/unit_conversion.cpp
    ${microtex_src}/box/box.cpp
    ${microtex_src}/box/box_factory.cpp
    ${microtex_src}/box/box_group.cpp
    ${microtex_src}/box/box_single.cpp
    ${microtex_src}/core/core.cpp
    ${microtex_src}/core/formula.cpp
    ${microtex_src}/core/formula_def.cpp
    ${microtex_src}/core/glue.cpp
    ${microtex_src}/core/localized_num.cpp
    ${microtex_src}/core/macro.cpp
    ${microtex_src}/core/macro_def.cpp
    ${microtex_src}/core/macro_impl.cpp
    ${microtex_src}/core/parser.cpp
    ${microtex_src}/fonts/alphabet.cpp
    ${microtex_src}/fonts/font_basic.cpp
    ${microtex_src}/fonts/font_info.cpp
    ${microtex_src}/fonts/fonts.cpp
    ${microtex_src}/utils/string_utils.cpp
    ${microtex_src}/utils/utf.cpp
    ${microtex_src}/utils/utils.cpp
    ${microtex_src}/res/builtin/formula_mappings.res.cpp
    ${microtex_src}/res/builtin/symbol_mapping.res.cpp
    ${microtex_src}/res/builtin/tex_param.res.cpp
    ${microtex_src}/res/builtin/tex_symbols.res.cpp
    ${microtex_src}/res/reg/builtin_font_reg.cpp
    ${microtex_src}/res/reg/builtin_syms_reg.cpp
    ${microtex_font_defs}
    ${microtex_sym_defs}
)

target_include_directories(microtex_oracle PRIVATE ${microtex_src})

if (MSVC)
    target_compile_options(microtex_oracle PRIVATE /utf-8 /W0 /bigobj /fp:precise)
    target_compile_definitions(microtex_oracle PRIVATE _USE_MATH_DEFINES NOMINMAX)
else()
    target_compile_options(microtex_oracle PRIVATE -w -ffp-contract=off)
endif()
