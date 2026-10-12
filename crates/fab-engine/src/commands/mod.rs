//! fabcraft's own command registry — a small, separate list alongside cadcraft's sealed one.
//! `crate::dispatch` is what merges the two for callers (the command line, menus, MCP).

pub mod bent_profile;
pub mod plate;
pub mod profile;
pub mod shared;

use cadcraft_engine::cmd::CommandSpec;

pub fn command_specs() -> &'static [CommandSpec] {
    static SPECS: std::sync::OnceLock<Vec<CommandSpec>> = std::sync::OnceLock::new();
    SPECS.get_or_init(|| {
        let mut v = Vec::new();
        v.extend(plate::specs());
        v.extend(profile::specs());
        v.extend(bent_profile::specs());
        v.extend(crate::fabdim::specs());
        v
    })
}

pub fn find_command(id: &str) -> Option<&'static CommandSpec> {
    let l = id.to_ascii_lowercase();
    command_specs().iter().find(|c| c.id == l)
}
