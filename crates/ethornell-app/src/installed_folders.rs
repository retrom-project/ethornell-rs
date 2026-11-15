//! Read-only installation snapshots supplied alongside a mounted game.
//! No registry file is executed and historical host paths are never opened.
use crate::file_policy;
use encoding_rs::{GBK, SHIFT_JIS};
use std::collections::BTreeSet;

const MAX_SNAPSHOTS: usize = 16;
const MAX_BYTES: u64 = 64 * 1024;
const MAX_RECORDS: usize = 64;

#[derive(Clone, Debug)]
struct Installation {
    company: String,
    product: String,
    folder: String,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct MountedInstallations {
    records: Vec<Installation>,
}

impl MountedInstallations {
    pub(crate) fn from_resources() -> Self {
        let Some(policy) = file_policy::active() else {
            return Self::default();
        };
        let Ok(names) = policy.enumerate("bgi-resource\\*.reg", false, MAX_SNAPSHOTS + 1, false)
        else {
            return Self::default();
        };
        if names.len() > MAX_SNAPSHOTS {
            return Self::default();
        }
        let mut records = Vec::new();
        for name in names {
            let path = format!("bgi-resource/{name}");
            if !policy
                .metadata(&path)
                .is_some_and(|m| m.is_file() && m.len() <= MAX_BYTES)
            {
                continue;
            }
            if let Ok(bytes) = policy.read_resource(&path, MAX_BYTES) {
                records.extend(parse_snapshot(&bytes));
                if records.len() > MAX_RECORDS {
                    return Self::default();
                }
            }
        }
        // A root-level snapshot may relocate this one installation. Multiple
        // distinct locations could describe optional components; never pretend
        // that all of those locations are the currently mounted resource tree.
        let folders: BTreeSet<_> = records.iter().map(|r| r.folder.as_str()).collect();
        if folders.len() != 1 {
            return Self::default();
        }
        Self { records }
    }

    pub(crate) fn contains(&self, company: &str, product: &str) -> bool {
        self.records
            .iter()
            .any(|r| same_ansi_name(&r.company, company) && same_ansi_name(&r.product, product))
    }
}

fn snapshot_text(bytes: &[u8]) -> Option<String> {
    if bytes.starts_with(&[255, 254]) {
        let data = bytes.get(2..)?;
        if data.len() % 2 != 0 {
            return None;
        }
        let words: Vec<_> = data
            .chunks_exact(2)
            .map(|b| u16::from_le_bytes([b[0], b[1]]))
            .collect();
        String::from_utf16(&words).ok()
    } else {
        std::str::from_utf8(bytes.strip_prefix(&[239, 187, 191]).unwrap_or(bytes))
            .ok()
            .map(str::to_owned)
    }
}

fn installation_key(line: &str) -> Option<(String, String)> {
    let key = line.strip_prefix('[')?.strip_suffix(']')?;
    let mut parts = key.split('\\');
    if !parts.next()?.eq_ignore_ascii_case("HKEY_LOCAL_MACHINE")
        || !parts.next()?.eq_ignore_ascii_case("SOFTWARE")
    {
        return None;
    }
    let mut company = parts.next()?;
    if company.eq_ignore_ascii_case("Wow6432Node") {
        company = parts.next()?;
    }
    let product = parts.next()?;
    if parts.next().is_some() || company.is_empty() || product.is_empty() {
        return None;
    }
    Some((company.into(), product.into()))
}

fn quoted_value(value: &str) -> Option<String> {
    let value = value.strip_prefix('"')?.strip_suffix('"')?;
    let mut chars = value.chars();
    let mut result = String::new();
    while let Some(c) = chars.next() {
        let c = if c == '\\' {
            match chars.next()? {
                '\\' => '\\',
                '"' => '"',
                _ => return None,
            }
        } else {
            c
        };
        if c.is_control() {
            return None;
        }
        result.push(c);
    }
    Some(result)
}

fn folder_value(line: &str) -> Option<String> {
    let (name, value) = line.split_once('=')?;
    if !name.trim().eq_ignore_ascii_case("\"InstalledFolder\"") {
        return None;
    }
    let value = quoted_value(value.trim())?.replace('/', "\\");
    let bytes = value.as_bytes();
    if !(bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && bytes[2] == b'\\')
        && !value.starts_with("\\\\")
    {
        return None;
    }
    if value.split('\\').any(|part| matches!(part, "." | "..")) {
        return None;
    }
    Some(value.trim_end_matches('\\').to_ascii_lowercase())
}

fn parse_snapshot(bytes: &[u8]) -> Vec<Installation> {
    let Some(text) = snapshot_text(bytes) else {
        return Vec::new();
    };
    let mut lines = text.lines();
    if !lines.next().is_some_and(|line| {
        matches!(
            line.trim(),
            "Windows Registry Editor Version 5.00" | "REGEDIT4"
        )
    }) {
        return Vec::new();
    }
    let mut records = Vec::new();
    let mut key = None;
    for line in lines.map(str::trim) {
        if line.starts_with('[') {
            key = installation_key(line);
        } else if let (Some((company, product)), Some(folder)) = (&key, folder_value(line)) {
            records.push(Installation {
                company: company.clone(),
                product: product.clone(),
                folder,
            });
            if records.len() > MAX_RECORDS {
                return records;
            }
        }
    }
    records
}

fn same_ansi_name(left: &str, right: &str) -> bool {
    let left = left.to_ascii_lowercase();
    let right = right.to_ascii_lowercase();
    if left == right {
        return true;
    }
    // The target uses RegOpenKeyExA, while .reg snapshots contain Unicode.
    // Preserve the underlying ANSI key bytes across the two supported BGI
    // code pages. Failed/lossy conversions never create a matching key.
    for left_encoding in [SHIFT_JIS, GBK] {
        let (a, _, a_errors) = left_encoding.encode(&left);
        if a_errors {
            continue;
        }
        for right_encoding in [SHIFT_JIS, GBK] {
            let (b, _, b_errors) = right_encoding.encode(&right);
            if !b_errors && a == b {
                return true;
            }
        }
    }
    false
}
