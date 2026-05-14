use anyhow::{bail, Result};
use inquire::{Confirm, CustomType, Select, Text};
use std::path::Path;

use crate::config::Config;
use crate::profiles;
use crate::proxmox::{self, cores_validator, sockets_validator};
use crate::size::SizeMb;

pub fn run(cfg: &Config, dry_run: bool) -> Result<()> {
    println!();

    let templates = proxmox::list_templates()?;
    if templates.is_empty() {
        println!("No templates found. Build one first.");
        return Ok(());
    }

    let labels: Vec<String> = templates
        .iter()
        .map(|(id, name)| format!("{id} — {name}"))
        .collect();
    let pick = Select::new("Template:", labels.clone()).prompt()?;
    let (template_id, _template_name) =
        templates[labels.iter().position(|x| x == &pick).unwrap()].clone();

    // Use Proxmox's own next-id endpoint as the default. Falls back to
    // template_id+1 only when pvesh isn't reachable.
    let default_vmid = proxmox::next_free_vmid().unwrap_or((template_id + 1).max(100));
    let new_id: u32 = CustomType::new("New VMID:")
        .with_default(default_vmid)
        .prompt()?;
    if proxmox::vmid_exists(new_id)? {
        bail!("VMID {new_id} already exists.");
    }
    let name = Text::new("VM name:").prompt()?;

    let storages = proxmox::list_storages().unwrap_or_default();
    let storage = if storages.is_empty() {
        Text::new("Storage pool:")
            .with_default(&cfg.storage)
            .prompt()?
    } else {
        let default_idx = storages.iter().position(|s| s == &cfg.storage).unwrap_or(0);
        Select::new("Storage pool:", storages.clone())
            .with_starting_cursor(default_idx)
            .prompt()?
    };

    let sockets: u32 = CustomType::new("CPU sockets:")
        .with_default(1u32)
        .with_validator(sockets_validator)
        .prompt()?;
    let cores: u32 = CustomType::new("Cores per socket:")
        .with_default(2u32)
        .with_validator(cores_validator)
        .prompt()?;
    let memory = CustomType::<SizeMb>::new("Memory (e.g. 2048M, 2G):")
        .with_default(SizeMb(2048))
        .with_help_message("Suffix with M, G, or T; bare number = MB")
        .prompt()?;
    let disk = CustomType::<SizeMb>::new("Disk size (e.g. 32G, 1T):")
        .with_default(SizeMb(32 * 1024))
        .with_help_message("Suffix with M, G, or T; bare number = MB")
        .prompt()?;

    let snippet_dir = Path::new(&cfg.snippet_dir);
    let snippet_path = profiles::pick_or_generate(snippet_dir, dry_run)?;
    let snippet_filename = snippet_path
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| anyhow::anyhow!("snippet path has no filename"))?
        .to_string();

    let start_after = Confirm::new("Start VM after creation?")
        .with_default(true)
        .prompt()?;

    println!();
    println!("Plan:");
    println!("  clone:       template {template_id} → vmid {new_id} ({name})");
    println!("  storage:     {storage}");
    let total_vcpus = sockets * cores;
    println!(
        "  resources:   {sockets} socket(s) × {cores} core(s) = {total_vcpus} vCPU, {memory} memory, {disk} disk"
    );
    println!(
        "  cloud-init:  {}:snippets/{}",
        cfg.snippet_storage, snippet_filename
    );
    println!("  start:       {start_after}");
    println!();
    if !Confirm::new("Proceed?").with_default(true).prompt()? {
        println!("Aborted.");
        return Ok(());
    }

    println!("→ Cloning template (full clone, can take a minute)...");
    proxmox::clone_template(template_id, new_id, &name, &storage, dry_run)?;

    println!("→ Applying VM settings + cloud-init snippet...");
    proxmox::apply_clone_settings(
        new_id,
        sockets,
        cores,
        memory.mb(),
        &cfg.snippet_storage,
        &snippet_filename,
        dry_run,
    )?;

    println!("→ Resizing disk to {disk}...");
    proxmox::resize_disk(new_id, disk.mb(), dry_run)?;

    if start_after {
        println!("→ Starting VM...");
        proxmox::start_vm(new_id, dry_run)?;
    }

    println!();
    if dry_run {
        println!("✓ Dry run complete — no changes applied.");
    } else {
        println!("✓ VM {new_id} ({name}) ready.");
        if start_after {
            println!("  Watch first boot: qm terminal {new_id}");
        } else {
            println!("  Start when ready: qm start {new_id}");
        }
    }
    Ok(())
}
