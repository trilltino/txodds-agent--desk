// Tauri's build script generates application context, embeds config, and wires
// platform-specific resource metadata before Cargo compiles the desktop binary.
fn main() {
    tauri_build::build()
}
