// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. If a
// copy of the MPL was not distributed with this file, You can obtain one at
// http://mozilla.org/MPL/2.0/. The original code is copyright (c) rasheed.

// Compiles the C++ shell against the open.mp SDK headers and links it into this library.
// It uses the compiler the server itself is built with on each target: MSVC on Windows,
// g++ -m32 on Linux.

fn main() {
    let sdk = "../../lib/open.mp-sdk";
    let version = |part: &str| std::env::var(format!("CARGO_PKG_VERSION_{part}")).unwrap();
    let msvc = std::env::var("CARGO_CFG_TARGET_ENV").unwrap() == "msvc";
    let x86 = matches!(
        std::env::var("CARGO_CFG_TARGET_ARCH").unwrap().as_str(),
        "x86" | "x86_64"
    );

    let mut build = cc::Build::new();
    build
        .cpp(true)
        .file("cpp/component.cpp")
        .include(format!("{sdk}/include"))
        .include(format!("{sdk}/lib/glm"))
        .include(format!("{sdk}/lib/robin-hood-hashing/src/include"))
        .include(format!("{sdk}/lib/span-lite/include"))
        .include(format!("{sdk}/lib/string-view-lite/include"))
        .include("../../lib/amx")
        .include("../../lib/amx/linux")
        .define("GLM_FORCE_QUAT_DATA_WXYZ", None)
        .define("NOMINMAX", None)
        .define("nssv_CONFIG_SELECT_STRING_VIEW", "nssv_STRING_VIEW_NONSTD")
        .define("span_CONFIG_SELECT_SPAN", "span_SPAN_NONSTD")
        .define("HAVE_STDINT_H", None)
        .define("HAVE_INTTYPES_H", None)
        .define("PJ_VERSION_MAJOR", version("MAJOR").as_str())
        .define("PJ_VERSION_MINOR", version("MINOR").as_str())
        .define("PJ_VERSION_PATCH", version("PATCH").as_str())
        .warnings(false);
    if x86 {
        build.define("GLM_FORCE_SSE2", None);
    }
    if msvc {
        build.flag("/std:c++17").flag("/fp:strict").flag("/EHsc");
        if std::env::var("CARGO_CFG_TARGET_POINTER_WIDTH").unwrap() == "32" {
            build.flag("/arch:SSE2");
        }
    } else {
        build
            .define("HAVE_UNISTD_H", None)
            .define("LINUX", None)
            .flag("-std=c++17");
        if x86 {
            build.flag("-msse2").flag("-mfpmath=sse");
        }
    }
    build.compile("pawn_json_shell");

    println!("cargo:rerun-if-changed=cpp/component.cpp");
    println!("cargo:rerun-if-changed=build.rs");
}
