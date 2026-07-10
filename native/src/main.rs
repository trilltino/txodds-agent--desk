// Binary entrypoint kept intentionally tiny. All Tauri setup, managed state,
// commands, and background tasks live in lib.rs so they can be tested/imported
// without duplicating application boot code.
fn main() {
    txodds_agent_desk_lib::run()
}
