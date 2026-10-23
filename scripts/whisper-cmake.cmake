# whisper.cpp runs this after each project() call (CMAKE_PROJECT_INCLUDE, see .cargo/config.toml).

# ggml's Vulkan backend builds its shader compiler as an external project, which CMake by default
# nests in <build>/ggml/src/ggml-vulkan/vulkan-shaders-gen-prefix/src/vulkan-shaders-gen-build.
# Under Cargo's target/<triple>/release/build/whisper-rs-sys-<hash>/out, MSBuild's files in that
# folder pass the Windows limit of 260 characters, and CMake finds no compiler. A short base
# keeps them about 40 characters shorter.
set_property(DIRECTORY PROPERTY EP_BASE "${CMAKE_BINARY_DIR}/ep")

# The cmake crate passes its own CMAKE_CXX_FLAGS, which replace CMake's defaults for Visual Studio
# and with them /EHsc. Without it, clang-cl on Arm64 turns C++ exceptions off, and MSVC on x64
# skips destructors while an exception unwinds. ggml reports a missing or old Vulkan driver with
# an exception. Each project() runs this file, so the flag is added only once.
if(MSVC AND NOT CMAKE_CXX_FLAGS MATCHES "/EHsc")
    string(APPEND CMAKE_CXX_FLAGS " /EHsc")
endif()
