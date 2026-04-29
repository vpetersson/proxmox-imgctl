//! Thin wrappers over `qm` and `pvesm`. We shell out instead of using the
//! Proxmox HTTP API because (a) we run on the node and (b) some operations
//! (`qm importdisk` in particular) don't have clean API equivalents.

use anyhow::{anyhow, bail, Context, Result};
use std::collections::HashSet;
use std::path::Path;
use std::process::{Command, Stdio};

fn run(args: &[&str]) -> Result<String> {
    let out = Command::new(args[0])
        .args(&args[1..])
        .stdin(Stdio::null())
        .output()
        .with_context(|| format!("spawning {}", args[0]))?;
    if !out.status.success() {
        bail!(
            "{} failed (status {}): {}",
            args.join(" "),
            out.status,
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Mutating-command runner: respects dry-run mode.
///
/// Dry-run prints the command instead of executing it. Read-only queries
/// (`qm list`, `pvesm status`, `pvesh get`) bypass this and call `run`
/// directly so menus can still be populated.
fn run_mut(args: &[&str], dry_run: bool) -> Result<()> {
    if dry_run {
        println!("[dry-run] would run: {}", quote_args(args));
        return Ok(());
    }
    run(args).map(|_| ())
}

/// Shell-style quoting for printing commands. Not used for execution.
fn quote_args(args: &[&str]) -> String {
    args.iter()
        .map(|a| {
            if a.is_empty()
                || a.chars()
                    .any(|c| c.is_whitespace() || "'\"\\$`".contains(c))
            {
                format!("'{}'", a.replace('\'', "'\\''"))
            } else {
                (*a).to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Ensure the requested VMID is free.
pub fn vmid_exists(vmid: u32) -> Result<bool> {
    let out = run(&["qm", "list"])?;
    for line in out.lines().skip(1) {
        let mut iter = line.split_whitespace();
        if let Some(id) = iter.next() {
            if id == vmid.to_string() {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

/// Returns (vmid, name) for every entry that is a template.
pub fn list_templates() -> Result<Vec<(u32, String)>> {
    // `qm list` doesn't expose template flag directly; use `pvesh` JSON.
    // `--output-format json` is stable across PVE 7/8/9.
    let out = run(&[
        "pvesh",
        "get",
        "/cluster/resources",
        "--type",
        "vm",
        "--output-format",
        "json",
    ])?;
    parse_template_list(&out)
}

/// Walk a JSON array/object, calling `f` for each top-level inner object.
/// Hand-rolled to avoid pulling in serde_json for one trivial use case.
fn for_each_object(json: &str, mut f: impl FnMut(&str)) {
    let bytes = json.as_bytes();
    let mut depth = 0i32;
    let mut start = 0usize;
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'{' => {
                if depth == 0 {
                    start = i;
                }
                depth += 1;
            }
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    f(&json[start..=i]);
                }
            }
            _ => {}
        }
    }
}

fn parse_template_list(json: &str) -> Result<Vec<(u32, String)>> {
    let mut out = Vec::new();
    for_each_object(json, |obj| {
        if extract_field(obj, "template").as_deref() == Some("1") {
            if let Some(vmid) = extract_field(obj, "vmid").and_then(|s| s.parse::<u32>().ok()) {
                let name = extract_field(obj, "name").unwrap_or_else(|| format!("vm{vmid}"));
                out.push((vmid, name));
            }
        }
    });
    out.sort_by_key(|(id, _)| *id);
    Ok(out)
}

/// Every VMID present on the cluster (templates + running VMs + stopped VMs).
fn list_all_vmids() -> Result<HashSet<u32>> {
    let raw = run(&[
        "pvesh",
        "get",
        "/cluster/resources",
        "--type",
        "vm",
        "--output-format",
        "json",
    ])?;
    let mut ids = HashSet::new();
    for_each_object(&raw, |obj| {
        if let Some(vmid) = extract_field(obj, "vmid").and_then(|s| s.parse::<u32>().ok()) {
            ids.insert(vmid);
        }
    });
    Ok(ids)
}

/// Next free VMID anywhere in the cluster, per Proxmox's own
/// `/cluster/nextid` endpoint. Starts at 100 (lowest valid).
pub fn next_free_vmid() -> Result<u32> {
    let raw = run(&["pvesh", "get", "/cluster/nextid", "--output-format", "text"])?;
    let s = raw.trim().trim_matches('"');
    s.parse::<u32>()
        .with_context(|| format!("parsing pvesh nextid output: {raw:?}"))
}

/// Lowest free VMID `>= start`, scanning the cluster's VM list. Falls back
/// to the cluster-wide nextid if everything `>= start` is somehow taken.
/// Used for templates, which conventionally live in the 9xxx range — there's
/// no API hint for "next free template id," so we scan.
pub fn next_free_vmid_from(start: u32) -> Result<u32> {
    let used = list_all_vmids()?;
    let mut candidate = start;
    while used.contains(&candidate) {
        match candidate.checked_add(1) {
            Some(next) => candidate = next,
            None => return next_free_vmid(),
        }
    }
    Ok(candidate)
}

/// Crude JSON field reader: handles "key":"value" and "key":number.
/// Sufficient for the well-formed output of pvesh.
fn extract_field(obj: &str, key: &str) -> Option<String> {
    let needle = format!("\"{key}\":");
    let idx = obj.find(&needle)?;
    let rest = &obj[idx + needle.len()..];
    let rest = rest.trim_start();
    if let Some(rest) = rest.strip_prefix('"') {
        let end = rest.find('"')?;
        Some(rest[..end].to_string())
    } else {
        let end = rest.find([',', '}']).unwrap_or(rest.len());
        Some(rest[..end].trim().to_string())
    }
}

/// Storage pools known to the cluster — names only.
pub fn list_storages() -> Result<Vec<String>> {
    let out = run(&["pvesm", "status"])?;
    let mut names = Vec::new();
    for line in out.lines().skip(1) {
        if let Some(name) = line.split_whitespace().next() {
            names.push(name.to_string());
        }
    }
    Ok(names)
}

/// Create a new VM shell with OVMF + secure boot, ready to receive an imported disk.
#[allow(clippy::too_many_arguments)]
pub fn create_vm_shell(
    vmid: u32,
    name: &str,
    storage: &str,
    bridge: &str,
    memory_mb: u32,
    cores: u32,
    ostype: &str,
    dry_run: bool,
) -> Result<()> {
    run_mut(
        &[
            "qm",
            "create",
            &vmid.to_string(),
            "--name",
            name,
            "--machine",
            "q35",
            "--bios",
            "ovmf",
            "--efidisk0",
            &format!("{storage}:0,efitype=4m,pre-enrolled-keys=1"),
            "--cpu",
            "host",
            "--cores",
            &cores.to_string(),
            "--sockets",
            "1",
            "--memory",
            &memory_mb.to_string(),
            "--net0",
            &format!("virtio,bridge={bridge}"),
            "--serial0",
            "socket",
            "--vga",
            "serial0",
            "--ostype",
            ostype,
            "--scsihw",
            "virtio-scsi-pci",
            "--agent",
            "enabled=1",
        ],
        dry_run,
    )
}

/// Import a local disk image into the VM's storage.
///
/// PVE 8.0 renamed `qm importdisk` to `qm disk import`. The old form was
/// kept as a deprecated alias through PVE 8.x and removed in PVE 9. We try
/// the modern form first and fall back to the old name if the binary on
/// this node doesn't recognize the new subcommand. This makes the tool
/// work unmodified on both PVE 8 and PVE 9.
pub fn import_disk(vmid: u32, image: &Path, storage: &str, dry_run: bool) -> Result<()> {
    let img = image
        .to_str()
        .ok_or_else(|| anyhow!("non-UTF8 image path: {}", image.display()))?;
    let vmid_s = vmid.to_string();

    if dry_run {
        // Show the command we'd actually try first; fallback only matters in real runs.
        println!(
            "[dry-run] would run: {}",
            quote_args(&["qm", "disk", "import", &vmid_s, img, storage])
        );
        return Ok(());
    }

    // Modern form (PVE 8.0+).
    let modern = run(&["qm", "disk", "import", &vmid_s, img, storage]);
    if modern.is_ok() {
        return Ok(());
    }
    // Fall back to the legacy form (PVE 7, kept as alias through PVE 8.x).
    // We only retry if the modern form looks like a CLI-syntax failure, not a
    // genuine import error — otherwise we'd mask real problems.
    let err = modern.unwrap_err();
    let msg = format!("{err:#}").to_lowercase();
    let looks_like_unknown_subcommand = msg.contains("unknown command")
        || msg.contains("unknown action")
        || msg.contains("400 parameter verification failed")
        || msg.contains("no such command")
        || msg.contains("usage:");
    if !looks_like_unknown_subcommand {
        return Err(err);
    }
    run(&["qm", "importdisk", &vmid_s, img, storage])?;
    Ok(())
}

/// After import, the cloud image lands at `vm-<vmid>-disk-1` because
/// efidisk0 already claimed `disk-0`.
pub fn attach_imported_disk_and_finalize_template(
    vmid: u32,
    storage: &str,
    dry_run: bool,
) -> Result<()> {
    let disk_ref = format!("{storage}:vm-{vmid}-disk-1");
    let vmid_s = vmid.to_string();
    run_mut(&["qm", "set", &vmid_s, "--virtio0", &disk_ref], dry_run)?;
    run_mut(&["qm", "set", &vmid_s, "--boot", "order=virtio0"], dry_run)?;
    run_mut(
        &[
            "qm",
            "set",
            &vmid_s,
            "--ide2",
            &format!("{storage}:cloudinit"),
        ],
        dry_run,
    )?;
    run_mut(&["qm", "template", &vmid_s], dry_run)?;
    Ok(())
}

pub fn clone_template(
    template_id: u32,
    new_id: u32,
    name: &str,
    storage: &str,
    dry_run: bool,
) -> Result<()> {
    run_mut(
        &[
            "qm",
            "clone",
            &template_id.to_string(),
            &new_id.to_string(),
            "--name",
            name,
            "--full",
            "--storage",
            storage,
        ],
        dry_run,
    )
}

pub fn apply_clone_settings(
    vmid: u32,
    cores: u32,
    memory_mb: u32,
    snippet_storage: &str,
    snippet_filename: &str,
    dry_run: bool,
) -> Result<()> {
    run_mut(
        &[
            "qm",
            "set",
            &vmid.to_string(),
            "--cores",
            &cores.to_string(),
            "--memory",
            &memory_mb.to_string(),
            "--balloon",
            "0",
            "--cicustom",
            &format!("user={snippet_storage}:snippets/{snippet_filename}"),
            "--ipconfig0",
            "ip=dhcp",
        ],
        dry_run,
    )
}

pub fn resize_disk(vmid: u32, gb: u32, dry_run: bool) -> Result<()> {
    run_mut(
        &[
            "qm",
            "resize",
            &vmid.to_string(),
            "virtio0",
            &format!("{gb}G"),
        ],
        dry_run,
    )
}

pub fn start_vm(vmid: u32, dry_run: bool) -> Result<()> {
    run_mut(&["qm", "start", &vmid.to_string()], dry_run)
}
