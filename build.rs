use std::{env, path::PathBuf, process::Command};
fn run(command: &mut Command) {
    let status = command.status().expect("native C compiler is required");
    assert!(
        status.success(),
        "native zgpui compilation failed: {command:?}"
    );
}
fn main() {
    if env::var_os("CARGO_FEATURE_NATIVE").is_none() {
        return;
    }
    let target = env::var("CARGO_CFG_TARGET_OS").unwrap();
    let software = target == "linux" && env::var_os("CARGO_FEATURE_LINUX_SOFTWARE").is_some();
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let platform = match target.as_str() {
        "linux" => "native/linux.c",
        "windows" => "native/windows.c",
        "macos" => "native/macos.m",
        _ => panic!("zgpui supports Linux, Windows and macOS"),
    };
    println!("cargo:rerun-if-changed=native");
    for variable in ["CC", "AR", "CFLAGS"] {
        println!("cargo:rerun-if-env-changed={variable}");
    }
    println!("cargo:rustc-link-search=native={}", out.display());
    let msvc = env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default() == "msvc";
    let mut objects = Vec::new();
    let draw = if software {
        "native/draw_software.c"
    } else {
        "native/draw.c"
    };
    for (index, source) in [draw, platform].iter().enumerate() {
        let object = out.join(format!("zgpui{index}.{}", if msvc { "obj" } else { "o" }));
        if msvc {
            run(Command::new("cl")
                .args(["/nologo", "/O2", "/W3", "/c", "/Inative", source])
                .arg(
                    if env::var("CARGO_CFG_TARGET_FEATURE")
                        .unwrap_or_default()
                        .split(',')
                        .any(|f| f == "crt-static")
                    {
                        "/MT"
                    } else {
                        "/MD"
                    },
                )
                .arg(format!("/Fo{}", object.display())));
        } else {
            let mut command = Command::new(env::var("CC").unwrap_or_else(|_| "cc".into()));
            command
                .args([
                    "-O2", "-fPIC", "-Wall", "-Wextra", "-Inative", "-c", source, "-o",
                ])
                .arg(&object);
            if software {
                command.arg("-DZGPUI_SOFTWARE");
            }
            if target == "linux" {
                let flags = Command::new("pkg-config")
                    .args(["--cflags", "freetype2", "fontconfig"])
                    .output()
                    .expect("pkg-config is required");
                assert!(
                    flags.status.success(),
                    "install FreeType and fontconfig development headers"
                );
                for flag in String::from_utf8(flags.stdout).unwrap().split_whitespace() {
                    command.arg(flag);
                }
            }
            if target == "macos" {
                command.arg("-DGL_SILENCE_DEPRECATION");
            }
            if let Ok(flags) = env::var("CFLAGS") {
                command.args(flags.split_whitespace());
            }
            run(&mut command);
        }
        objects.push(object);
    }
    if msvc {
        run(Command::new("lib")
            .arg("/nologo")
            .arg(format!("/OUT:{}", out.join("zgpui_native.lib").display()))
            .args(&objects));
    } else {
        run(Command::new(env::var("AR").unwrap_or_else(|_| "ar".into()))
            .arg("crs")
            .arg(out.join("libzgpui_native.a"))
            .args(&objects));
    }
    println!("cargo:rustc-link-lib=static=zgpui_native");
    match target.as_str() {
        "linux" => {
            for name in ["X11", "freetype", "fontconfig", "m"] {
                println!("cargo:rustc-link-lib={name}");
            }
            if !software {
                println!("cargo:rustc-link-lib=GL");
            }
        }
        "windows" => {
            for name in ["user32", "gdi32", "opengl32", "kernel32"] {
                println!("cargo:rustc-link-lib={name}");
            }
        }
        "macos" => {
            for name in ["AppKit", "OpenGL", "CoreText", "CoreGraphics", "Foundation"] {
                println!("cargo:rustc-link-lib=framework={name}");
            }
        }
        _ => unreachable!(),
    }
}
