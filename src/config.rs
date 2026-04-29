use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

const CONFIG_PATH: &str = "/etc/proxmox-imgctl.toml";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Storage pool VM disks land on (e.g. "local-lvm").
    pub storage: String,
    /// Storage pool that holds cloud-init snippets (must support snippets, e.g. "local").
    pub snippet_storage: String,
    /// Filesystem path where snippets are written for the snippet_storage.
    pub snippet_dir: String,
    /// Network bridge for VM NICs.
    pub bridge: String,
    /// Local cache directory for downloaded cloud images.
    pub cache_dir: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            storage: "local-lvm".into(),
            snippet_storage: "local".into(),
            snippet_dir: "/var/lib/vz/snippets".into(),
            bridge: "vmbr0".into(),
            cache_dir: "/var/lib/proxmox-imgctl/cache".into(),
        }
    }
}

const DEFAULT_TEMPLATE: &str = r#"# proxmox-imgctl configuration
# Edit values to match your node, then re-run the tool.

# Storage pool VM disks land on.
storage = "local-lvm"

# Storage pool used for cloud-init snippets. Must have content "snippets" enabled
# in /etc/pve/storage.cfg. The default "local" usually does.
snippet_storage = "local"

# Filesystem path where snippets are written. Must match snippet_storage.
snippet_dir = "/var/lib/vz/snippets"

# Default network bridge for VM NICs.
bridge = "vmbr0"

# Local cache directory for downloaded cloud images.
cache_dir = "/var/lib/proxmox-imgctl/cache"
"#;

pub fn load_or_init(dry_run: bool) -> Result<Config> {
    let path = Path::new(CONFIG_PATH);
    if !path.exists() {
        if dry_run {
            eprintln!(
                "[dry-run] {CONFIG_PATH} missing — using defaults (would seed the file in a real run)."
            );
            return Ok(Config::default());
        }
        fs::write(path, DEFAULT_TEMPLATE)
            .with_context(|| format!("writing default config to {CONFIG_PATH}"))?;
        eprintln!("Wrote default config to {CONFIG_PATH} — review it and re-run.");
    }
    let raw = fs::read_to_string(path).with_context(|| format!("reading {CONFIG_PATH}"))?;
    let cfg: Config = toml::from_str(&raw).with_context(|| format!("parsing {CONFIG_PATH}"))?;
    Ok(cfg)
}
