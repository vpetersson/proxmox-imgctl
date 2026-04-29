# proxmox-imgctl

Interactive cloud-image template + VM bootstrapper for Proxmox VE.

`proxmox-imgctl` is a small Rust binary you run on a Proxmox node. It walks
you through downloading a verified cloud image, building a Proxmox template
from it (OVMF + secure boot, cloud-init drive, virtio everywhere), and then
cloning that template into ready-to-boot VMs with cloud-init snippets, all
through a single interactive menu.

It shells out to `qm`, `pvesm`, and `pvesh` rather than talking to the
Proxmox HTTP API, because (a) it's designed to run on the node itself and
(b) `qm disk import` has no clean API equivalent. The wrapper layer is thin
and easy to swap if you ever want to make it run remotely.

## Features

- Curated catalog of Ubuntu (26.04, 24.04, 22.04) and Debian (13, 12, 11) cloud images, checksum-verified against the upstream `SHA256SUMS` / `SHA512SUMS`.
- Built-in cloud-init profiles (`minimal`, `dev`, `docker`) plus an interactive generator and "load existing snippet" option.
- Auto-picks the next free VMID from `pvesh /cluster/nextid` (templates default to the lowest free VMID >= 9000).
- `--dry-run` mode prints every command, snippet, and download that would be performed without executing or writing anything.
- Works on both PVE 8.x and PVE 9.x — handles the `qm importdisk` -> `qm disk import` rename transparently.
- One static binary, no Python or Perl runtime.

## Requirements

- Proxmox VE 8.x or 9.x.
- Run as `root` on the Proxmox node (not as a regular user).
- A storage pool with `snippets` content type enabled. The default `local` storage usually has this; if not, run `pvesm set local --content snippets,iso,vztmpl,backup,images,rootdir` (or edit `/etc/pve/storage.cfg`).
- Outbound HTTPS access from the node to `cloud-images.ubuntu.com` and `cloud.debian.org`.

## Installation

### Option 1 — Prebuilt binary (recommended)

A static `x86_64-linux-musl` binary is attached to every tagged release.
On the Proxmox node, as root:

```bash
# Download binary + checksum sidecar to the current directory
curl -fsSLO https://github.com/vpetersson/proxmox-imgctl/releases/latest/download/proxmox-imgctl-x86_64-linux
curl -fsSLO https://github.com/vpetersson/proxmox-imgctl/releases/latest/download/proxmox-imgctl-x86_64-linux.sha256

# Verify (the .sha256 file references the binary by its plain name, so both
# must sit side-by-side when you run sha256sum -c)
sha256sum -c proxmox-imgctl-x86_64-linux.sha256

# Install
install -m 0755 proxmox-imgctl-x86_64-linux /usr/local/bin/proxmox-imgctl
```

The musl build has no glibc dependency, so the same binary runs on PVE 8 (Debian 12 / glibc 2.36) and PVE 9 (Debian 13 / glibc 2.41) without rebuild.

### Option 2 — Build from source

```bash
# On the Proxmox node
apt-get update
apt-get install -y curl build-essential pkg-config git
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable
. "$HOME/.cargo/env"

git clone https://github.com/vpetersson/proxmox-imgctl
cd proxmox-imgctl
cargo build --release
install -m 0755 target/release/proxmox-imgctl /usr/local/bin/
```

## Configuration

On first run as root, `proxmox-imgctl` seeds `/etc/proxmox-imgctl.toml` with
sensible defaults and exits so you can review them. The defaults:

```toml
# Storage pool VM disks land on.
storage = "local-lvm"

# Storage pool used for cloud-init snippets. Must have content "snippets"
# enabled in /etc/pve/storage.cfg. The default "local" usually does.
snippet_storage = "local"

# Filesystem path where snippets are written. Must match snippet_storage.
snippet_dir = "/var/lib/vz/snippets"

# Default network bridge for VM NICs.
bridge = "vmbr0"

# Local cache directory for downloaded cloud images.
cache_dir = "/var/lib/proxmox-imgctl/cache"
```

Adjust as needed and re-run.

## Usage

Launch the menu:

```bash
sudo proxmox-imgctl
```

Two top-level actions: **Build template from cloud image** and **Spawn VM from template**.

### Example — build a Ubuntu 24.04 template

```
$ sudo proxmox-imgctl
? What do you want to do?  Build template from cloud image
? Base image:  Ubuntu 24.04 (noble)
✓ Cached image at /var/lib/proxmox-imgctl/cache/ubuntu-24.04-noble.img matches checksum.
? Storage pool:  local-lvm
? Network bridge: vmbr0
? Template VMID: 9000
? Template name: ubuntu-noble-template
? Memory (MB): 1024
? CPU cores: 1

Plan:
  base image:  https://cloud-images.ubuntu.com/noble/current/noble-server-cloudimg-amd64.img -> /var/lib/proxmox-imgctl/cache/ubuntu-24.04-noble.img
  vmid:        9000
  name:        ubuntu-noble-template
  storage:     local-lvm
  bridge:      vmbr0
  resources:   1 core(s), 1024 MB

? Proceed? Yes
-> Creating VM shell...
-> Importing disk (this can take a minute)...
-> Attaching disk, configuring boot, adding cloud-init drive, marking as template...

✓ Template 9000 (ubuntu-noble-template) ready.
```

### Example — spawn a VM from the template

```
$ sudo proxmox-imgctl
? What do you want to do?  Spawn VM from template
? Template:  9000 — ubuntu-noble-template
? New VMID: 101
? VM name: web-01
? Storage pool:  local-lvm
? CPU cores: 2
? Memory (MB): 2048
? Disk size (GB): 32
? Cloud-init profile:  [builtin] dev — user + git, build-essential, curl, vim, htop, tmux, jq
? Primary username: admin
? ssh_import_id (e.g. gh:octocat, blank to skip): gh:octocat
? Reboot after first-boot config? No
? Snippet filename: dev.yaml
Wrote /var/lib/vz/snippets/dev.yaml
? Start VM after creation? Yes

Plan:
  clone:       template 9000 -> vmid 101 (web-01)
  storage:     local-lvm
  resources:   2 core(s), 2048 MB, 32 GB disk
  cloud-init:  local:snippets/dev.yaml
  start:       true

? Proceed? Yes
-> Cloning template (full clone, can take a minute)...
-> Applying VM settings + cloud-init snippet...
-> Resizing disk to 32 GB...
-> Starting VM...

✓ VM 101 (web-01) ready.
  Watch first boot: qm terminal 101
```

### Dry-run

`--dry-run` (or `-n`) prints every action that would be taken — `qm`
commands, snippet writes, image downloads — without executing or touching
the filesystem. Read-only queries (`qm list`, `pvesm status`, `pvesh get`)
still execute so the menus can populate.

```bash
sudo proxmox-imgctl --dry-run
```

Sample dry-run output for a Build Template flow:

```
[dry-run] would download https://cloud-images.ubuntu.com/noble/current/noble-server-cloudimg-amd64.img
          -> /var/lib/proxmox-imgctl/cache/ubuntu-24.04-noble.img
          (verify Sha256 from https://cloud-images.ubuntu.com/noble/current/SHA256SUMS)
[dry-run] would run: qm create 9000 --name 'ubuntu-noble-template' --machine q35 --bios ovmf ...
[dry-run] would run: qm disk import 9000 /var/lib/proxmox-imgctl/cache/ubuntu-24.04-noble.img local-lvm
[dry-run] would run: qm set 9000 --virtio0 'local-lvm:vm-9000-disk-1'
[dry-run] would run: qm set 9000 --boot order=virtio0
[dry-run] would run: qm set 9000 --ide2 'local-lvm:cloudinit'
[dry-run] would run: qm template 9000

✓ Dry run complete — no changes applied.
```

When `--dry-run` is set, the binary doesn't require root, doesn't seed
`/etc/proxmox-imgctl.toml`, and doesn't write snippets — useful for
previewing from a non-root account or even off the Proxmox node.

## Image catalog

| Distro | Release | Codename | Format |
|---|---|---|---|
| Ubuntu | 26.04 LTS | resolute | `.img` (qcow2) |
| Ubuntu | 24.04 LTS | noble | `.img` (qcow2) |
| Ubuntu | 22.04 LTS | jammy | `.img` (qcow2) |
| Debian | 13 | trixie | `.qcow2` |
| Debian | 12 | bookworm | `.qcow2` |
| Debian | 11 | bullseye | `.qcow2` |

Each entry resolves through the upstream `current`/`latest` symlink, so a
fresh download always grabs the latest point release. Checksums are pulled
from the published `SHA256SUMS` (Ubuntu) or `SHA512SUMS` (Debian) and
verified before import.

To add a distro, append an entry to `CATALOG` in `src/catalog.rs`.

## Cloud-init profiles

Three built-in `#cloud-config` profiles ship with the tool:

- **`minimal`** — single user with passwordless sudo + bash shell. No extra packages.
- **`dev`** — minimal, plus `git`, `build-essential`, `curl`, `vim`, `htop`, `tmux`, `jq`, `ca-certificates`.
- **`docker`** — minimal, plus the official Docker convenience installer (`get.docker.com`) and the user added to the `docker` group.

All three prompt for the primary username and an optional `ssh_import_id`
(e.g. `gh:octocat` to import keys from GitHub, `lp:octocat` for Launchpad).
Snippets are written to `snippet_dir` (default `/var/lib/vz/snippets/`) and
referenced via `qm set --cicustom user=<storage>:snippets/<file>`.

You can also pick **`[generate interactively]`** to build a profile from
scratch (custom packages, runcmd, reboot-after) or **`[existing] foo.yaml`**
to reuse anything already in the snippets directory.

## PVE 8 vs PVE 9

The disk-import subcommand was renamed in PVE 8.0:

| PVE version | Subcommand |
|---|---|
| PVE 7 | `qm importdisk` |
| PVE 8.x | `qm disk import` (preferred); `qm importdisk` retained as deprecated alias |
| PVE 9 | `qm disk import` only |

`proxmox-imgctl` tries the modern form first and falls back to the legacy
name only when the modern form fails with a CLI-syntax error — real import
failures (storage full, bad image) are propagated as-is. A single binary
works unchanged on both PVE 8 and 9.

## Known limitations

- **amd64 only.** No arm64 release builds yet.
- **Hardcoded image catalog.** Adding a distro requires a recompile.
- **Imported disk is assumed to land at `vm-<vmid>-disk-1`.** This is correct because the EFI disk claims `disk-0` first, but it's not parsed from `qm disk import` output. Multi-disk templates would require changes here.
- **No "delete" or "edit" actions.** Build template / spawn VM are the only two top-level actions; cleanup is via `qm destroy` / `qm stop` directly.
- **Single-node mindset.** The tool doesn't yet help you target a specific node when run on a cluster.

## Development

```bash
cargo fmt --all
cargo clippy --all-targets -- -D warnings
cargo build --release
```

CI (`.github/workflows/ci.yml`) runs `fmt --check`, `clippy -D warnings`,
and `build + test` on every push to `master` and on every pull request.

Tagged releases (`git tag v0.1.0 && git push --tags`) trigger
`.github/workflows/release.yml`, which builds the static musl binary and
attaches it to a GitHub release with a SHA256 sidecar.

## License

MIT.
