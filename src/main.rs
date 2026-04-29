use anyhow::Result;
use inquire::Select;

mod catalog;
mod config;
mod download;
mod profiles;
mod proxmox;
mod spawn;
mod template;

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        print_help();
        return Ok(());
    }
    let dry_run = args.iter().any(|a| a == "--dry-run" || a == "-n");
    let unknown: Vec<&str> = args
        .iter()
        .filter(|a| !matches!(a.as_str(), "--dry-run" | "-n" | "--help" | "-h"))
        .map(String::as_str)
        .collect();
    if !unknown.is_empty() {
        eprintln!("unknown argument(s): {}", unknown.join(" "));
        print_help();
        std::process::exit(2);
    }

    if dry_run {
        eprintln!("⚠ DRY RUN — no VMs, snippets, or downloads will be created.");
    } else if !is_root() {
        eprintln!("proxmox-imgctl must be run as root on a Proxmox node.");
        eprintln!("(Use --dry-run to preview without root.)");
        std::process::exit(1);
    }

    let cfg = config::load_or_init(dry_run)?;

    loop {
        let action = Select::new(
            "What do you want to do?",
            vec![
                "Build template from cloud image",
                "Spawn VM from template",
                "Quit",
            ],
        )
        .prompt()?;

        match action {
            "Build template from cloud image" => template::run(&cfg, dry_run)?,
            "Spawn VM from template" => spawn::run(&cfg, dry_run)?,
            _ => break,
        }
    }
    Ok(())
}

fn print_help() {
    println!(
        "proxmox-imgctl {}\n\
         Interactive cloud-image template + VM bootstrapper for Proxmox.\n\n\
         USAGE:\n  \
             proxmox-imgctl [--dry-run] [--help]\n\n\
         OPTIONS:\n  \
             -n, --dry-run    Show planned actions without executing or writing anything\n  \
             -h, --help       Print this help",
        env!("CARGO_PKG_VERSION")
    );
}

fn is_root() -> bool {
    // SAFETY: getuid is always safe to call.
    unsafe { libc_getuid() == 0 }
}

extern "C" {
    #[link_name = "getuid"]
    fn libc_getuid() -> u32;
}
