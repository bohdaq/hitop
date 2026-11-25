//! Just enough zip for collection folders: reading stored and deflated text files (bounded), writing
//! stored ones.
use std::collections::BTreeMap;

const MAX_TOTAL: usize = 25 * 1024 * 1024;
const MAX_ENTRIES: usize = 5000;

fn u16_at(b: &[u8], i: usize) -> Option<usize> {
    Some(u16::from_le_bytes(b.get(i..i.checked_add(2)?)?.try_into().ok()?) as usize)
}

fn u32_at(b: &[u8], i: usize) -> Option<usize> {
    Some(u32::from_le_bytes(b.get(i..i.checked_add(4)?)?.try_into().ok()?) as usize)
}

/// The text files by path (`/`), as the central directory lists them; files that aren't UTF-8, links'
/// targets and macOS's own are left out.
pub fn unzip(b: &[u8]) -> Result<BTreeMap<String, String>, String> {
    let bad = || "This zip can't be read.".to_string();
    // The end record: 22 bytes and a comment of up to 64 KiB.
    let from = b.len().saturating_sub(22 + 65_535);
    let end = (from..b.len().saturating_sub(21)).rev().find(|i| b[*i..].starts_with(b"PK\x05\x06")).ok_or_else(bad)?;
    let count = u16_at(b, end + 10).ok_or_else(bad)?;
    let mut at = u32_at(b, end + 16).ok_or_else(bad)?;
    if count > MAX_ENTRIES {
        return Err("The zip has too many files.".into());
    }
    let mut out = BTreeMap::new();
    let mut total = 0usize;
    for _ in 0..count {
        let h = b.get(at..).filter(|h| h.starts_with(b"PK\x01\x02")).ok_or_else(bad)?;
        let get16 = |i| u16_at(h, i).ok_or_else(bad);
        let get32 = |i| u32_at(h, i).ok_or_else(bad);
        let (flags, method, packed, size) = (get16(8)?, get16(10)?, get32(20)?, get32(24)?);
        let (name_len, extra, comment, local) = (get16(28)?, get16(30)?, get16(32)?, get32(42)?);
        let link = (get32(38)? >> 16) & 0o170000 == 0o120000;
        let name = h.get(46..46 + name_len).ok_or_else(bad)?;
        at += 46 + name_len + extra + comment;
        let Ok(name) = std::str::from_utf8(name) else { continue };
        let name = name.replace('\\', "/");
        if name.ends_with('/') || link || name.starts_with("__MACOSX/") || name.starts_with('/') || name.split('/').any(|p| p == "..") {
            continue;
        }
        if flags & 1 != 0 {
            return Err("The zip is encrypted.".into());
        }
        total = total.checked_add(size).filter(|t| *t <= MAX_TOTAL).ok_or("The zip is too large.")?;
        let l = b.get(local..).filter(|l| l.starts_with(b"PK\x03\x04")).ok_or_else(bad)?;
        let start = 30 + u16_at(l, 26).ok_or_else(bad)? + u16_at(l, 28).ok_or_else(bad)?;
        let data = l.get(start..start.checked_add(packed).ok_or_else(bad)?).ok_or_else(bad)?;
        // The size the directory gives is the limit, whatever the data would grow to.
        let bytes = match method {
            0 if packed == size => data.to_vec(),
            8 => miniz_oxide::inflate::decompress_to_vec_with_limit(data, size).map_err(|_| bad())?,
            _ => return Err(bad()),
        };
        if let Ok(text) = String::from_utf8(bytes) {
            out.insert(name, text);
        }
    }
    Ok(out)
}

fn crc32(data: &[u8]) -> u32 {
    !data.iter().fold(!0u32, |mut c, b| {
        c ^= u32::from(*b);
        for _ in 0..8 {
            c = (c >> 1) ^ (0xEDB8_8320 & (c & 1).wrapping_neg());
        }
        c
    })
}

/// Stored, dated 2026-01-01, so the same files are the same bytes.
pub fn zip(files: &BTreeMap<String, String>) -> Vec<u8> {
    let mut out = vec![];
    let mut dir = vec![];
    let date: u16 = (46 << 9) | (1 << 5) | 1;
    for (name, text) in files {
        let offset = out.len() as u32;
        let mut common = vec![];
        for v in [20u16, 0x0800, 0, 0, date] {
            common.extend(v.to_le_bytes());
        }
        for v in [crc32(text.as_bytes()), text.len() as u32, text.len() as u32] {
            common.extend(v.to_le_bytes());
        }
        common.extend((name.len() as u16).to_le_bytes());
        common.extend([0, 0]);
        out.extend(b"PK\x03\x04");
        out.extend(&common);
        out.extend(name.as_bytes());
        out.extend(text.as_bytes());
        dir.extend(b"PK\x01\x02");
        dir.extend(20u16.to_le_bytes());
        dir.extend(&common);
        dir.extend([0u8; 10]);
        dir.extend(offset.to_le_bytes());
        dir.extend(name.as_bytes());
    }
    let (start, n) = (out.len() as u32, files.len() as u16);
    out.extend(&dir);
    out.extend(b"PK\x05\x06\0\0\0\0");
    out.extend(n.to_le_bytes());
    out.extend(n.to_le_bytes());
    out.extend((dir.len() as u32).to_le_bytes());
    out.extend(start.to_le_bytes());
    out.extend([0, 0]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn written_zips_read_back_and_other_tools_agree() {
        let files: BTreeMap<String, String> = [("hitop.yaml".to_string(), "format: 1\n".to_string()), ("requests/ä.yaml".to_string(), String::new())].into();
        let z = zip(&files);
        assert_eq!(unzip(&z).unwrap(), files);
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
        for n in 0..z.len() {
            let _ = unzip(&z[..n]);
        }
        assert!(unzip(b"PK").is_err());
    }

    #[test]
    fn deflated_bounded_and_confined() {
        // One deflated entry, its declared size smaller than its data: refused.
        let make = |name: &str, size: u32| {
            let data = miniz_oxide::deflate::compress_to_vec(&[b'a'; 1000], 6);
            let mut z = b"PK\x03\x04".to_vec();
            z.extend([0; 22]);
            z.extend((name.len() as u16).to_le_bytes());
            z.extend([0, 0]);
            z.extend(name.as_bytes());
            z.extend(&data);
            let at = z.len() as u32;
            z.extend(b"PK\x01\x02");
            z.extend([0; 6]);
            z.extend([8, 0]);
            z.extend([0; 8]);
            z.extend((data.len() as u32).to_le_bytes());
            z.extend(size.to_le_bytes());
            z.extend((name.len() as u16).to_le_bytes());
            z.extend([0; 16]);
            z.extend(name.as_bytes());
            let len = z.len() as u32 - at;
            z.extend(b"PK\x05\x06\0\0\0\0\x01\0\x01\0");
            z.extend(len.to_le_bytes());
            z.extend(at.to_le_bytes());
            z.extend([0, 0]);
            z
        };
        assert_eq!(unzip(&make("a.yaml", 1000)).unwrap()["a.yaml"].len(), 1000);
        assert!(unzip(&make("a.yaml", 10)).is_err());
        assert!(unzip(&make("a.yaml", 100_000_000)).unwrap_err().contains("too large"));
        assert!(unzip(&make("../a.yaml", 1000)).unwrap().is_empty());
    }
}
