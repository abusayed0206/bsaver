fn main() {
    // Embed the application icon as a Win32 resource into all binaries.
    // build.rs runs once per package (not per binary), so there's no way
    // to conditionally target only BanglaSaver. Having the icon in bsaver.exe
    // too is harmless — screensavers don't display a titlebar.
    let mut res = winresource::WindowsResource::new();
    res.set_icon("assets/BanglaSaver.ico");
    res.compile().expect("Failed to compile Windows resources");
}
