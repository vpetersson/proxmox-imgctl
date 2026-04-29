use anyhow::{bail, Result};
use inquire::{Confirm, CustomType, Select, Text};
use std::path::Path;

use crate::config::Config;
use crate::profiles;
use crate::proxmox;

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

    let new_id: u32 = CustomType::new("New VMID:")
        .with_default((template_id + 1).max(100))
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

    let cores: u32 = CustomType::new("CPU cores:").with_default(2u32).prompt()?;
    let memory: u32 = CustomType::new("Memory (MB):")
        .with_default(2048u32)
        .prompt()?;
    let disk_gb: u32 = CustomType::new("Disk size (GB):")
        .with_default(32u32)
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
    println!("  resources:   {cores} core(s), {memory} MB, {disk_gb} GB disk");
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
        cores,
        memory,
        &cfg.snippet_storage,
        &snippet_filename,
        dry_run,
    )?;

    println!("→ Resizing disk to {disk_gb} GB...");
    proxmox::resize_disk(new_id, disk_gb, dry_run)?;

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
