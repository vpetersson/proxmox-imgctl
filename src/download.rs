use anyhow::{anyhow, bail, Context, Result};
use indicatif::{ProgressBar, ProgressStyle};
use sha2::{Digest, Sha256, Sha512};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::catalog::{HashAlgo, Image};

/// Download `image` to `cache_dir` if not already present + verified.
/// Returns the local path of the verified image.
///
/// In dry-run, no network or filesystem I/O happens; we just compute and
/// return the would-be destination path.
pub fn fetch(image: &Image, cache_dir: &Path, dry_run: bool) -> Result<PathBuf> {
    let dest = cache_dir.join(image.cache_filename());

    if dry_run {
        println!(
            "[dry-run] would download {} → {} (verify {:?} from {})",
            image.image_url,
            dest.display(),
            image.sums_algo,
            image.sums_url,
        );
        return Ok(dest);
    }

    fs::create_dir_all(cache_dir)
        .with_context(|| format!("creating cache dir {}", cache_dir.display()))?;

    let expected = fetch_expected_hash(image)?;

    if dest.exists() {
        let actual = hash_file(&dest, image.sums_algo)?;
        if actual.eq_ignore_ascii_case(&expected) {
            println!("✓ Cached image at {} matches checksum.", dest.display());
            return Ok(dest);
        }
        println!("⚠ Cached image checksum mismatch — re-downloading.");
        fs::remove_file(&dest).ok();
    }

    download_with_progress(image.image_url, &dest)?;

    let actual = hash_file(&dest, image.sums_algo)?;
    if !actual.eq_ignore_ascii_case(&expected) {
        fs::remove_file(&dest).ok();
        bail!("Checksum mismatch after download.\n  expected: {expected}\n  actual:   {actual}",);
    }
    println!("✓ Verified {} ({:?})", dest.display(), image.sums_algo);
    Ok(dest)
}

fn fetch_expected_hash(image: &Image) -> Result<String> {
    let body = http_client()?
        .get(image.sums_url)
        .send()
        .with_context(|| format!("GET {}", image.sums_url))?
        .error_for_status()?
        .text()?;
    parse_sums(&body, image.sums_filename).ok_or_else(|| {
        anyhow!(
            "filename {} not found in {}",
            image.sums_filename,
            image.sums_url
        )
    })
}

/// SUMS files are lines of `<hex>  <filename>` (Debian uses `*<filename>` for binary mode).
fn parse_sums(body: &str, target: &str) -> Option<String> {
    for line in body.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.splitn(2, char::is_whitespace);
        let hash = parts.next()?.trim();
        let rest = parts.next()?.trim_start();
        let name = rest.strip_prefix('*').unwrap_or(rest);
        if name == target {
            return Some(hash.to_string());
        }
    }
    None
}

fn hash_file(path: &Path, algo: HashAlgo) -> Result<String> {
    let mut f = File::open(path).with_context(|| format!("opening {}", path.display()))?;
    let mut buf = vec![0u8; 1024 * 1024];
    match algo {
        HashAlgo::Sha256 => {
            let mut h = Sha256::new();
            loop {
                let n = f.read(&mut buf)?;
                if n == 0 {
                    break;
                }
                h.update(&buf[..n]);
            }
            Ok(hex::encode(h.finalize()))
        }
        HashAlgo::Sha512 => {
            let mut h = Sha512::new();
            loop {
                let n = f.read(&mut buf)?;
                if n == 0 {
                    break;
                }
                h.update(&buf[..n]);
            }
            Ok(hex::encode(h.finalize()))
        }
    }
}

fn download_with_progress(url: &str, dest: &Path) -> Result<()> {
    let mut resp = http_client()?
        .get(url)
        .send()
        .with_context(|| format!("GET {url}"))?
        .error_for_status()?;

    let total = resp.content_length().unwrap_or(0);
    let bar = if total > 0 {
        let b = ProgressBar::new(total);
        b.set_style(
            ProgressStyle::with_template(
                "{spinner} {bytes}/{total_bytes} [{wide_bar}] {bytes_per_sec} ETA {eta}",
            )
            .unwrap()
            .progress_chars("=>-"),
        );
        b
    } else {
        let b = ProgressBar::new_spinner();
        b.enable_steady_tick(Duration::from_millis(120));
        b
    };
    bar.set_message(format!("downloading {url}"));

    let tmp = dest.with_extension("part");
    let mut out = File::create(&tmp).with_context(|| format!("creating {}", tmp.display()))?;
    let mut buf = vec![0u8; 1024 * 1024];
    loop {
        let n = resp.read(&mut buf)?;
        if n == 0 {
            break;
        }
        out.write_all(&buf[..n])?;
        bar.inc(n as u64);
    }
    out.flush()?;
    drop(out);
    fs::rename(&tmp, dest)
        .with_context(|| format!("renaming {} -> {}", tmp.display(), dest.display()))?;
    bar.finish_with_message("download complete");
    Ok(())
}

fn http_client() -> Result<reqwest::blocking::Client> {
    reqwest::blocking::Client::builder()
        .user_agent(concat!("proxmox-imgctl/", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(60 * 30))
        .build()
        .map_err(Into::into)
}
