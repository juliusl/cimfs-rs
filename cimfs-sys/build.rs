extern crate bindgen;

use std::env;
use std::path::PathBuf;

fn main() {
    // `cimfs.lib` should be included with windows
    println!("cargo:rustc-link-lib=cimfs");

    let bindings = bindgen::Builder::default()
        .header("wrapper.h")
        .blocklist_file("windows.h")
        .blocklist_file("winuser.h")
        // **Note**
        // TLS - is a special static thread local storage implementation in windows for PE assemblies
        // Currently bindgen has a bug that will generate broken code for theses types. 
        // Since these are a niche type, these are ignored since they aren't directly referenced by `CimFs.h`.
        //
        .blocklist_type("IMAGE_TLS_DIRECTORY")
        .blocklist_type("PIMAGE_TLS_DIRECTORY")
        .blocklist_type("IMAGE_TLS_DIRECTORY64")
        .blocklist_type("PIMAGE_TLS_DIRECTORY64")
        .blocklist_type("_IMAGE_TLS_DIRECTORY64")
        .blocklist_type("tagMONITORINFOEXA")
        .blocklist_type("tagMONITORINFOEXW")
        .blocklist_type("MONITORINFOEXA")
        .blocklist_type("MONITORINFOEXW")
        .blocklist_type("LPMONITORINFOEXA")
        .blocklist_type("LPMONITORINFOEXW")
        .blocklist_type("MONITORINFOEX")
        .blocklist_type("LPMONITORINFOEX")
        .derive_default(true)
        .derive_debug(true)
        .derive_copy(true)
        .derive_eq(true)
        .derive_ord(true)
        .derive_partialeq(true)
        .derive_partialord(true)
        .derive_hash(true)
        .generate()
        .expect("should be able to create bindings");

    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("should be abel to write bindings");
}