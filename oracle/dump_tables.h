#pragma once

#include <ostream>

namespace oracle {

void DumpTables(std::ostream &out);
void FontQueries(std::ostream &out);
void EnvQueries(std::ostream &out);
void PredefinedCache(std::ostream &out);

} // namespace oracle
