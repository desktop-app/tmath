# tmath

LaTeX math layout for Telegram Desktop in safe Rust.

The engine takes a TeX formula and returns its metrics and a flat list of
drawing calls. The host paints that list, with Qt in Telegram Desktop. The
engine crate forbids `unsafe`. The C ABI crate holds the only `unsafe` code,
and only what a C interface needs.

tmath is a port of [MicroTeX](https://github.com/NanoMichael/MicroTeX), by way
of our fork [desktop-app/MicroTeX](https://github.com/desktop-app/MicroTeX). For
every formula it produces exactly what that fork produces. It carries
MicroTeX's tables and built-in definitions, so MicroTeX's license notice is
kept in [NOTICE](NOTICE).

The C interface is [include/tmath.h](include/tmath.h), built by the `tmath-c`
crate in `ffi/`. [replay/tmath_replay.cpp](replay/tmath_replay.cpp) paints the
display list through MicroTeX's `Graphics2D` and measures text the way MicroTeX
does, for hosts that keep MicroTeX's Qt platform code.

See [PLAN.md](PLAN.md) for the design and the milestones, and
[FIXES.md](FIXES.md) for the fork's fixes this replaces.

tmath is released under the [MIT License](LICENSE).
