use std::{env, path::PathBuf, process::Command};

fn run(command: &mut Command) {
    let status = command
        .status()
        .expect("a C compiler is required for Linux fonts");
    assert!(
        status.success(),
        "zgpui-raster compilation failed: {command:?}"
    );
}

fn main() {
    if env::var_os("CARGO_FEATURE_FONT").is_none()
        || env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("linux")
    {
        return;
    }
    for path in ["native.c", "../native/font_linux.h", "../native/zgpui.h"] {
        println!("cargo:rerun-if-changed={path}");
    }
    for variable in [
        "CC",
        "AR",
        "CFLAGS",
        "PKG_CONFIG",
        "PKG_CONFIG_PATH",
        "PKG_CONFIG_LIBDIR",
    ] {
        println!("cargo:rerun-if-env-changed={variable}");
    }
    let out = PathBuf::from(env::var_os("OUT_DIR").expect("Cargo provides OUT_DIR"));
    let flags = Command::new(env::var("PKG_CONFIG").unwrap_or_else(|_| "pkg-config".into()))
        .args(["--cflags", "freetype2", "fontconfig"])
        .output()
        .expect("pkg-config is required for Linux fonts");
    assert!(
        flags.status.success(),
        "install FreeType and fontconfig development headers"
    );
    let mut compiler = Command::new(env::var("CC").unwrap_or_else(|_| "cc".into()));
    compiler
        .args([
            "-std=c11",
            "-D_DEFAULT_SOURCE",
            "-O2",
            "-fPIC",
            "-Wall",
            "-Wextra",
            "-Werror",
            "-Wno-misleading-indentation",
            "-I../native",
            "-c",
            "native.c",
            "-o",
        ])
        .arg(out.join("raster_font.o"));
    compiler.args(
        String::from_utf8(flags.stdout)
            .expect("UTF-8 compiler flags")
            .split_whitespace(),
    );
    if let Ok(flags) = env::var("CFLAGS") {
        compiler.args(flags.split_whitespace());
    }
    run(&mut compiler);
    run(Command::new(env::var("AR").unwrap_or_else(|_| "ar".into()))
        .arg("crs")
        .arg(out.join("libzgpui_raster_font.a"))
        .arg(out.join("raster_font.o")));
    println!("cargo:rustc-link-search=native={}", out.display());
    println!("cargo:rustc-link-lib=static=zgpui_raster_font");
    println!("cargo:rustc-link-lib=freetype");
    println!("cargo:rustc-link-lib=fontconfig");
    println!("cargo:rustc-link-lib=m");
}
