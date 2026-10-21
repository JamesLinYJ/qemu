// SPDX-License-Identifier: MIT
use std::{env, fs, path::PathBuf, process::Command};

#[derive(Debug)]
struct MemoryNames;

impl bindgen::callbacks::ParseCallbacks for MemoryNames {
    fn item_name(&self, item: bindgen::callbacks::ItemInfo<'_>) -> Option<String> {
        if !matches!(item.kind, bindgen::callbacks::ItemKind::Type) {
            return None;
        }
        Some(
            item.name
                .split('_')
                .filter(|part| !part.is_empty())
                .map(|part| {
                    let mut chars = part.chars();
                    chars.next().unwrap().to_uppercase().to_string() + chars.as_str()
                })
                .collect(),
        )
    }
}

fn main() {
    println!("cargo:rerun-if-env-changed=POCKET_QEMU_BUILD");
    let source = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap())
        .join("../..")
        .canonicalize()
        .unwrap();
    let build = env::var_os("POCKET_QEMU_BUILD")
        .map(PathBuf::from)
        .unwrap_or_else(|| source.join("build-pocket-tci"))
        .canonicalize()
        .expect("build QEMU first");
    let config = fs::read_to_string(build.join("config-host.h")).unwrap();
    let backend = if config
        .lines()
        .any(|line| line.starts_with("#define CONFIG_TCG_INTERPRETER"))
    {
        "tci"
    } else {
        panic!("Validation requires upstream TCI; native TCG JIT is not allowed");
    };
    let target = fs::read_to_string(build.join("aarch64-softmmu-config-target.h")).unwrap();
    assert!(
        !target.contains("#define CONFIG_HVF") && !config.contains("#define CONFIG_HVF"),
        "HVF is not allowed"
    );
    println!("cargo:rustc-env=POCKET_INTERPRETER={backend}");
    println!("cargo:rerun-if-changed={}/config-host.h", build.display());
    let flags = Command::new("pkg-config")
        .args(["--cflags", "glib-2.0"])
        .output()
        .expect("pkg-config is required");
    assert!(
        flags.status.success(),
        "GLib development headers are required"
    );
    let mut bindings = bindgen::Builder::default()
        .header_contents(
            "pocket-bindings-input.h",
            concat!(
                "#include \"qemu/osdep.h\"\n",
                "#include \"system/system.h\"\n",
                "#include \"qemu/main-loop.h\"\n",
                "#include \"system/replay.h\"\n"
            ),
        )
        .clang_arg(format!("-I{}", build.display()))
        .clang_arg(format!("-I{}/include", source.display()))
        .clang_arg(format!("-I{}", source.display()))
        .allowlist_function("qemu_(init|main_loop|cleanup)")
        .allowlist_function("bql_unlock|replay_mutex_unlock")
        .allowlist_recursively(false)
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .layout_tests(false)
        .generate_comments(false);
    for flag in String::from_utf8(flags.stdout).unwrap().split_whitespace() {
        bindings = bindings.clang_arg(flag);
    }
    bindings
        .generate()
        .expect("generate interfaces from actual QEMU headers")
        .write_to_file(PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("qemu.rs"))
        .unwrap();
    // Use the public Mach declarations from the selected Apple SDK. Keep this
    // generated separately so no handwritten structure mirrors an SDK ABI.
    bindgen::Builder::default()
        .header_contents("pocket-memory-input.h", "#include <mach/mach.h>\n")
        .allowlist_function("^task_info$")
        .allowlist_type("^task_vm_info$")
        .allowlist_var("^(TASK_VM_INFO|mach_task_self_)$")
        .parse_callbacks(Box::new(MemoryNames))
        .layout_tests(false)
        .generate_comments(false)
        .generate()
        .expect("generate public Mach memory interfaces")
        .write_to_file(PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("memory.rs"))
        .unwrap();
    println!("cargo:rustc-link-search=native={}", build.display());
    println!("cargo:rustc-link-lib=dylib=qemu-aarch64-softmmu");
    println!("cargo:rustc-link-arg=-Wl,-rpath,{}", build.display());
}
