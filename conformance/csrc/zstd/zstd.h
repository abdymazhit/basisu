// Shim so the vendored basisu_transcoder.cpp's `#include "../zstd/zstd.h"`
// resolves to the system zstd when BASISD_SUPPORT_KTX2_ZSTD is enabled by
// build.rs. The build adds the system zstd include dir to the search path.
#pragma once
#include <zstd.h>
