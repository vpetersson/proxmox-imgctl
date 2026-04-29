//! Hardcoded catalog of supported cloud images.
//!
//! Each entry points at the upstream "current"/"latest" alias so a fresh
//! download always grabs the latest point release. Checksum is verified
//! against the published SHA256SUMS file.

#[derive(Clone, Copy, Debug)]
pub enum HashAlgo {
    Sha256,
    Sha512,
}

#[derive(Clone, Debug)]
pub struct Image {
    pub distro: &'static str,
    pub release: &'static str,
    pub codename: &'static str,
    pub image_url: &'static str,
    /// URL of the published SUMS file (SHA256SUMS or SHA512SUMS).
    pub sums_url: &'static str,
    pub sums_algo: HashAlgo,
    /// Filename to look up inside the SUMS file.
    pub sums_filename: &'static str,
    /// Default ostype string for `qm set --ostype`.
    pub ostype: &'static str,
}

impl Image {
    pub fn label(&self) -> String {
        format!("{} {} ({})", self.distro, self.release, self.codename)
    }

    /// Local cache filename — embeds distro/release so multiple coexist.
    pub fn cache_filename(&self) -> String {
        let ext = if self.image_url.ends_with(".qcow2") {
            "qcow2"
        } else {
            "img"
        };
        format!(
            "{}-{}-{}.{ext}",
            self.distro.to_lowercase(),
            self.release,
            self.codename,
        )
    }
}

pub static CATALOG: &[Image] = &[
    Image {
        distro: "Ubuntu",
        release: "26.04",
        codename: "resolute",
        image_url:
            "https://cloud-images.ubuntu.com/resolute/current/resolute-server-cloudimg-amd64.img",
        sums_url: "https://cloud-images.ubuntu.com/resolute/current/SHA256SUMS",
        sums_algo: HashAlgo::Sha256,
        sums_filename: "resolute-server-cloudimg-amd64.img",
        ostype: "l26",
    },
    Image {
        distro: "Ubuntu",
        release: "24.04",
        codename: "noble",
        image_url: "https://cloud-images.ubuntu.com/noble/current/noble-server-cloudimg-amd64.img",
        sums_url: "https://cloud-images.ubuntu.com/noble/current/SHA256SUMS",
        sums_algo: HashAlgo::Sha256,
        sums_filename: "noble-server-cloudimg-amd64.img",
        ostype: "l26",
    },
    Image {
        distro: "Ubuntu",
        release: "22.04",
        codename: "jammy",
        image_url: "https://cloud-images.ubuntu.com/jammy/current/jammy-server-cloudimg-amd64.img",
        sums_url: "https://cloud-images.ubuntu.com/jammy/current/SHA256SUMS",
        sums_algo: HashAlgo::Sha256,
        sums_filename: "jammy-server-cloudimg-amd64.img",
        ostype: "l26",
    },
    Image {
        distro: "Debian",
        release: "13",
        codename: "trixie",
        image_url:
            "https://cloud.debian.org/images/cloud/trixie/latest/debian-13-generic-amd64.qcow2",
        sums_url: "https://cloud.debian.org/images/cloud/trixie/latest/SHA512SUMS",
        sums_algo: HashAlgo::Sha512,
        sums_filename: "debian-13-generic-amd64.qcow2",
        ostype: "l26",
    },
    Image {
        distro: "Debian",
        release: "12",
        codename: "bookworm",
        image_url:
            "https://cloud.debian.org/images/cloud/bookworm/latest/debian-12-generic-amd64.qcow2",
        sums_url: "https://cloud.debian.org/images/cloud/bookworm/latest/SHA512SUMS",
        sums_algo: HashAlgo::Sha512,
        sums_filename: "debian-12-generic-amd64.qcow2",
        ostype: "l26",
    },
    Image {
        distro: "Debian",
        release: "11",
        codename: "bullseye",
        image_url:
            "https://cloud.debian.org/images/cloud/bullseye/latest/debian-11-generic-amd64.qcow2",
        sums_url: "https://cloud.debian.org/images/cloud/bullseye/latest/SHA512SUMS",
        sums_algo: HashAlgo::Sha512,
        sums_filename: "debian-11-generic-amd64.qcow2",
        ostype: "l26",
    },
];
