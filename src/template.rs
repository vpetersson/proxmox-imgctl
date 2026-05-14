use anyhow::{bail, Result};
use inquire::{Confirm, CustomType, Select, Text};
use std::path::Path;

use crate::catalog::CATALOG;
use crate::config::Config;
use crate::download;
use crate::proxmox::{self, cores_validator, sockets_validator};
use crate::size::SizeMb;

pub fn run(cfg: &Config, dry_run: bool) -> Result<()> {
    println!();

    let labels: Vec<String> = CATALOG.iter().map(|i| i.label()).collect();
    let pick = Select::new("Base image:", labels.clone()).prompt()?;
    let image = &CATALOG[labels.iter().position(|x| x == &pick).unwrap()];

    let cache_dir = Path::new(&cfg.cache_dir);
    let local = download::fetch(image, cache_dir, dry_run)?;

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

    let bridge = Text::new("Network bridge:")
        .with_default(&cfg.bridge)
        .prompt()?;

    // Templates conventionally live in the 9xxx range. Pick the lowest free
    // VMID >= 9000; fall back to a hardcoded 9000 only if pvesh is unreachable
    // (e.g. running on a non-Proxmox box for dry-run testing).
    let default_vmid = proxmox::next_free_vmid_from(9000).unwrap_or(9000);
    let vmid: u32 = CustomType::new("Template VMID:")
        .with_default(default_vmid)
        .with_error_message("Enter a valid VMID (positive integer)")
        .prompt()?;
    if proxmox::vmid_exists(vmid)? {
        bail!("VMID {vmid} already exists — pick another or remove it first.");
    }

    let default_name = format!(
        "{}-{}-template",
        image.distro.to_lowercase(),
        image.codename
    );
    let name = Text::new("Template name:")
        .with_default(&default_name)
        .prompt()?;

    let memory = CustomType::<SizeMb>::new("Memory (e.g. 1024M, 2G):")
        .with_default(SizeMb(1024))
        .with_help_message("Suffix with M, G, or T; bare number = MB; must convert exactly")
        .prompt()?;
    let sockets: u32 = CustomType::new("CPU sockets:")
        .with_default(1u32)
        .with_validator(sockets_validator)
        .prompt()?;
    let cores: u32 = CustomType::new("Cores per socket:")
        .with_default(1u32)
        .with_validator(cores_validator)
        .prompt()?;

    println!();
    println!("Plan:");
    println!("  base image:  {} → {}", image.image_url, local.display());
    println!("  vmid:        {vmid}");
    println!("  name:        {name}");
    println!("  storage:     {storage}");
    println!("  bridge:      {bridge}");
    let total_vcpus = sockets * cores;
    println!(
        "  resources:   {sockets} socket(s) × {cores} core(s) = {total_vcpus} vCPU, {memory} memory"
    );
    println!();
    if !Confirm::new("Proceed?").with_default(true).prompt()? {
        println!("Aborted.");
        return Ok(());
    }

    println!("→ Creating VM shell...");
    proxmox::create_vm_shell(
        vmid,
        &name,
        &storage,
        &bridge,
        memory.mb(),
        sockets,
        cores,
        image.ostype,
        dry_run,
    )?;

    println!("→ Importing disk (this can take a minute)...");
    proxmox::import_disk(vmid, &local, &storage, dry_run)?;

    println!("→ Attaching disk, configuring boot, adding cloud-init drive, marking as template...");
    proxmox::attach_imported_disk_and_finalize_template(vmid, &storage, dry_run)?;

    println!();
    if dry_run {
        println!("✓ Dry run complete — no changes applied.");
    } else {
        println!("✓ Template {vmid} ({name}) ready.");
    }
    Ok(())
}
