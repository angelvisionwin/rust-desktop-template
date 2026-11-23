fn main() {
    #[cfg(target_os = "windows")]
    {
        let mut resource = winresource::WindowsResource::new();
        resource.set_icon("assets/app.ico");
        resource.set(
            "FileDescription",
            "Batería: estado de batería en la bandeja",
        );
        resource.set("ProductName", "Batería");
        resource.set("LegalCopyright", "MPL-2.0");
        resource.set("OriginalFilename", "bateria.exe");
        if let Err(error) = resource.compile() {
            println!("cargo:warning=no se pudo incrustar el icono: {error}");
        }
    }
}
