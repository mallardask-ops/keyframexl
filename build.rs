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

    #[cfg(target_os = "windows")]
    {
        println!("cargo:rerun-if-changed=assets/icon.ico");
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon.ico");
        res.set("ProductName", "KeyFrameXL");
        res.set("FileDescription", "KeyFrameXL — Pattern Sequencer & DAW");
        if let Err(e) = res.compile() {
            println!("cargo:warning=winresource failed: {e}");
        }
    }
}
