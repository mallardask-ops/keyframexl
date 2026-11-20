// Signalsmith Stretch (vendor/signalsmith, MIT): the sampler's time stretch.
fn main() {
    println!("cargo:rerun-if-changed=vendor/signalsmith");
    cc::Build::new()
        .cpp(true)
        .std("c++17")
        .include("vendor/signalsmith")
        .file("vendor/signalsmith/wrapper.cpp")
        .opt_level(3)
        .warnings(false)
        .compile("kfx_stretch");
}
