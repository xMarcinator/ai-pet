// Builds the inspection shim (src/inspect.cpp) against the rive-cpp that rive-rs compiles. rive-cpp's no-op
// factory lives outside its src/, which rive-rs doesn't build, so it is compiled here.
fn main() {
    let rive_cpp = std::path::PathBuf::from("vendor/rive-rs/submodules/rive-cpp");
    println!("cargo:rerun-if-changed=src/inspect.cpp");
    cc::Build::new()
        .cpp(true)
        .std("c++17")
        .include(rive_cpp.join("include"))
        .include(&rive_cpp)
        .file("src/inspect.cpp")
        .file(rive_cpp.join("utils/no_op_factory.cpp"))
        .define("_RIVE_INTERNAL_", None)
        .warnings(false)
        .compile("spike-inspect");
}
