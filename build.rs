use std::{env, fs, path::PathBuf, process::Command};

fn main() {
    println!("cargo:rerun-if-changed=assets/app-icon.ico");
    println!("cargo:rerun-if-env-changed=WINDRES");
    println!("cargo:rerun-if-env-changed=RC");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let root = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let source = out.join("app-icon.rc");
    let icon = root.join("assets/app-icon.ico");
    fs::write(
        &source,
        format!("1 ICON \"{}\"\n", icon.to_string_lossy().replace('\\', "/")),
    )
    .expect("write Windows icon resource");
    let gnu = env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("gnu");
    let resource = out.join(if gnu { "app-icon.o" } else { "app-icon.res" });
    let mut compiler = if gnu {
        Command::new(env::var_os("WINDRES").unwrap_or_else(|| "windres".into()))
    } else {
        Command::new(env::var_os("RC").unwrap_or_else(|| "rc".into()))
    };
    if gnu {
        compiler
            .arg("-i")
            .arg(&source)
            .args(["-O", "coff", "-o"])
            .arg(&resource);
    } else {
        compiler
            .arg("/nologo")
            .arg("/fo")
            .arg(&resource)
            .arg(&source);
    }
    let status = compiler
        .status()
        .expect("run Windows resource compiler (windres or rc)");
    assert!(status.success(), "Windows icon resource compilation failed");
    println!(
        "cargo:rustc-link-arg-bin=niimbot-d101={}",
        resource.display()
    );
}
