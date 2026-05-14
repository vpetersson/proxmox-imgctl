//! Human-friendly size input. Accepts plain numbers (interpreted as MB) or a
//! suffix: `M`/`MB`/`MiB`, `G`/`GB`/`GiB`, `T`/`TB`/`TiB`. Case-insensitive.
//! Internally everything is normalized to MB so it round-trips into Proxmox's
//! `--memory` / `qm resize ...M` arguments without ambiguity.

use std::fmt;
use std::str::FromStr;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SizeMb(pub u32);

impl SizeMb {
    pub fn mb(self) -> u32 {
        self.0
    }
}

impl FromStr for SizeMb {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.trim();
        if s.is_empty() {
            return Err("size cannot be empty".into());
        }

        let split = s.find(|c: char| c.is_ascii_alphabetic()).unwrap_or(s.len());
        let (num_part, suffix) = s.split_at(split);
        let num_part = num_part.trim();
        let suffix = suffix.trim().to_ascii_uppercase();

        let n: f64 = num_part
            .parse()
            .map_err(|_| format!("not a number: {num_part:?}"))?;
        if !n.is_finite() || n < 0.0 {
            return Err(format!("size must be non-negative: {num_part}"));
        }

        let mult_mb: f64 = match suffix.as_str() {
            "" | "M" | "MB" | "MIB" => 1.0,
            "G" | "GB" | "GIB" => 1024.0,
            "T" | "TB" | "TIB" => 1024.0 * 1024.0,
            other => {
                return Err(format!(
                    "unknown unit {other:?} — use M, G, or T (e.g. 512M, 2G, 1T)"
                ))
            }
        };

        // Check the exact value before rounding so sub-MB inputs like `0.5M`
        // don't round up past the minimum.
        let mb_exact = n * mult_mb;
        if mb_exact < 1.0 {
            return Err("size must be at least 1 MB".into());
        }
        if mb_exact > u32::MAX as f64 {
            return Err("size too large".into());
        }
        // Reject inputs that don't convert to a whole number of MB (e.g.
        // `1.4G` = 1433.6 MB). Fractions that land exactly on an MB
        // boundary like `1.5G` = 1536 MB still go through.
        if mb_exact.fract() != 0.0 {
            return Err(format!(
                "{s:?} isn't a whole number of MB ({mb_exact} MB) — pick a value that converts exactly"
            ));
        }
        Ok(SizeMb(mb_exact as u32))
    }
}

impl fmt::Display for SizeMb {
    /// Compact form: prefer T → G → M, whichever divides evenly.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mb = self.0;
        const G: u32 = 1024;
        const T: u32 = 1024 * 1024;
        if mb >= T && mb.is_multiple_of(T) {
            write!(f, "{}T", mb / T)
        } else if mb >= G && mb.is_multiple_of(G) {
            write!(f, "{}G", mb / G)
        } else {
            write!(f, "{mb}M")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[track_caller]
    fn ok(s: &str, mb: u32) {
        assert_eq!(s.parse::<SizeMb>().unwrap().mb(), mb, "input: {s}");
    }

    #[test]
    fn parses_plain_number_as_mb() {
        ok("1024", 1024);
        ok(" 2048 ", 2048);
    }

    #[test]
    fn parses_suffixes() {
        ok("512M", 512);
        ok("512MB", 512);
        ok("512MiB", 512);
        ok("2G", 2048);
        ok("2gb", 2048);
        ok("2 gib", 2048);
        ok("1T", 1024 * 1024);
    }

    #[test]
    fn parses_fractional() {
        ok("1.5G", 1536);
    }

    #[test]
    fn rejects_garbage() {
        assert!("".parse::<SizeMb>().is_err());
        assert!("G".parse::<SizeMb>().is_err());
        assert!("-1G".parse::<SizeMb>().is_err());
        assert!("1X".parse::<SizeMb>().is_err());
        assert!("0M".parse::<SizeMb>().is_err());
    }

    #[test]
    fn rejects_sub_mb_even_when_rounding_would_lift_it() {
        // 0.5M is below the minimum; it must not silently round up to 1M.
        assert!("0.5M".parse::<SizeMb>().is_err());
        assert!("0.4M".parse::<SizeMb>().is_err());
    }

    #[test]
    fn rejects_non_integral_mb() {
        // `1.4G` is 1433.6 MB — no whole-MB representation; reject rather
        // than silently rounding.
        assert!("1.4G".parse::<SizeMb>().is_err());
        assert!("1.5M".parse::<SizeMb>().is_err());
        assert!("2.7G".parse::<SizeMb>().is_err());
    }

    #[test]
    fn display_picks_largest_clean_unit() {
        assert_eq!(SizeMb(512).to_string(), "512M");
        assert_eq!(SizeMb(2048).to_string(), "2G");
        assert_eq!(SizeMb(1024 * 1024).to_string(), "1T");
        assert_eq!(SizeMb(1500).to_string(), "1500M");
    }
}
