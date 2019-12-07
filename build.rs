extern crate bindgen;
extern crate cc;

use std::env;
use std::process::Command;

use rand::Rng;
use std::fs::{remove_file, File};
use std::io::Write;
use std::path::PathBuf;

fn add_def(v: &mut Vec<(String, String)>, key: &str, val: &str) {
    v.push((key.to_owned(), val.to_owned()));
}

fn main() {
    let mut defines = Vec::new();
    for i in &[
        "size_t",
        "unsigned int",
        "unsigned long",
        "unsigned long long",
    ] {
        let def_name = format!("SIZEOF_{}", i.to_uppercase().replace(" ", "_"));
        defines.push((def_name, check_native_size(i)));
    }
    add_def(&mut defines, "SECONDARY_DJW", "1");
    add_def(&mut defines, "SECONDARY_FGK", "1");
    add_def(&mut defines, "EXTERNAL_COMPRESSION", "0");
    add_def(&mut defines, "XD3_USE_LARGEFILE64", "1");

    #[cfg(windows)]
    add_def(&mut defines, "XD3_WIN32", "1");
    add_def(&mut defines, "SHELL_TESTS", "0");

    {
        let mut builder = cc::Build::new();
        builder.include("xdelta3/xdelta3");
        for (key, val) in &defines {
            builder.define(&key, Some(val.as_str()));
        }

        builder
            .file("xdelta3/xdelta3/xdelta3.c")
            .warnings(false)
            .compile("xdelta3");
    }

    {
        let mut builder = bindgen::Builder::default();

        for (key, val) in &defines {
            builder = builder.clang_arg(format!("-D{}={}", key, val));
        }
        let bindings = builder
            .header("xdelta3/xdelta3/xdelta3.h")
            .parse_callbacks(Box::new(bindgen::CargoCallbacks))
            .whitelist_function("xd3_.*")
            .generate()
            .expect("Unable to generate bindings");

        // Write the bindings to the $OUT_DIR/bindings.rs file.
        let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
        bindings
            .write_to_file(out_path.join("bindings.rs"))
            .expect("Couldn't write bindings!");
    }
}

fn check_native_size(name: &str) -> String {
    let builder = cc::Build::new();
    let out_dir = env::var("OUT_DIR").unwrap();
    let compiler = builder.get_compiler();
    let mut compile = Command::new(compiler.path().as_os_str());
    let test_code = format!("#include <stdint.h>\n#include <stdio.h>\nint main() {{printf(\"%lu\", sizeof({})); return 0;}}\n", name);
    // didn't use tempfile since tempfile was having issues on Windows
    let mut rng = rand::thread_rng();
    let test_binary_fn = format!("{}/test-{}", out_dir, rng.gen::<i32>());

    #[cfg(windows)]
    let test_binary_fn = format!("{}.exe", test_binary_fn);

    let test_source_fn = format!("{}/test-{:x}.c", out_dir, rng.gen::<i32>());
    let mut test_source = File::create(&test_source_fn).expect("Error creating test compile files");

    compile.args(compiler.args()).current_dir(out_dir);
    if compiler.is_like_msvc() {
        compile.args(&[&test_source_fn, &format!("/Fe{}", test_binary_fn)]);
    } else {
        compile.args(&[&test_source_fn, "-o", &test_binary_fn]);
    }
    test_source
        .write_all(test_code.as_bytes())
        .expect("Error writing test compile files");
    drop(test_source); // close the source file, otherwise there will be problems on Windows
    for &(ref a, ref b) in compiler.env().iter() {
        compile.env(a, b);
    }
    compile.output().expect("Error compiling test source");
    remove_file(test_source_fn).ok();

    compile = Command::new(&test_binary_fn);
    let output = compile
        .output()
        .expect("Error executing test binary")
        .stdout;
    let output = String::from_utf8(output).expect("Error converting Unicode sequence");
    remove_file(test_binary_fn).ok();
    return output;
}
