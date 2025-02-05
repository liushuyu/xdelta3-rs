extern crate bindgen;
extern crate cc;

use std::env;

use std::path::PathBuf;

fn add_def(v: &mut Vec<(String, String)>, key: &str, val: &str) {
    v.push((key.to_owned(), val.to_owned()));
}

fn main() {
    let mut defines = Vec::new();
    
    add_def(&mut defines, "SIZEOF_SIZE_T", &format!("{}", size_of::<usize>()));
    add_def(&mut defines, "SIZEOF_UNSIGNED_INT", &format!("{}", size_of::<std::ffi::c_uint>()));
    add_def(&mut defines, "SIZEOF_UNSIGNED_LONG", &format!("{}", size_of::<std::ffi::c_ulong>()));
    add_def(&mut defines, "SIZEOF_UNSIGNED_LONG_LONG", &format!("{}", size_of::<std::ffi::c_ulonglong>()));
    
    
    add_def(&mut defines, "SECONDARY_DJW", "1");
    add_def(&mut defines, "SECONDARY_FGK", "1");
    add_def(&mut defines, "EXTERNAL_COMPRESSION", "0");
    add_def(&mut defines, "XD3_USE_LARGEFILE64", "1");

    #[cfg(windows)]
    add_def(&mut defines, "XD3_WIN32", "1");
    add_def(&mut defines, "SHELL_TESTS", "0");

    let mut builder = cc::Build::new();

    #[cfg(feature = "lzma")]
    {
        add_def(&mut defines, "SECONDARY_LZMA", "1");
        let liblzma = pkg_config::Config::new().probe("liblzma").unwrap();
        builder.includes(&liblzma.include_paths);
    }

    {
        builder.include("xdelta3/xdelta3");
        builder.std("c11");
        builder.define("static_assert", Some("_Static_assert"));
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
        builder = builder.clang_arg("--std=c11");
        builder = builder.clang_arg("-Dstatic_assert=_Static_assert");
        for (key, val) in &defines {
            builder = builder.clang_arg(format!("-D{}={}", key, val));
        }

        let bindings = builder
            .header("xdelta3/xdelta3/xdelta3.h")
            .parse_callbacks(Box::new(
                bindgen::CargoCallbacks::new().rerun_on_header_files(false),
            ))
            .allowlist_function("xd3_.*")
            .allowlist_type("xd3_.*")
            .rustified_enum("xd3_.*")
            .generate()
            .expect("Unable to generate bindings");

        // Write the bindings to the $OUT_DIR/bindings.rs file.
        let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
        bindings
            .write_to_file(out_path.join("bindings.rs"))
            .expect("Couldn't write bindings!");
    }
}