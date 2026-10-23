# CMake toolchain of whisper.cpp for Windows on Arm64, see scripts/build-windows.sh. ggml, its
# math library, builds for Arm64 only with clang-cl, Visual Studio's Clang. The cmake crate always
# passes -Thost=x64 and offers no other way to choose the toolset, and a toolchain file may
# replace it. CMake reads it before it applies the toolset.
set(CMAKE_GENERATOR_TOOLSET "ClangCL,host=x64")

# Without a toolchain file, the cmake crate describes the target itself when it differs from the
# build computer. It skips that when one is given, so this file does the same.
if(NOT CMAKE_HOST_SYSTEM_PROCESSOR STREQUAL "ARM64")
    set(CMAKE_SYSTEM_NAME Windows)
    set(CMAKE_SYSTEM_PROCESSOR ARM64)
endif()
