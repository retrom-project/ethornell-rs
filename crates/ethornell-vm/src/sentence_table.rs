//! Versioned native sentence table, discovered from the confirmed ._bsi wire
//! format. No assets, filenames, translated sentences or game-specific keys.
use std::collections::BTreeMap;
const MAX_BYTES: usize = 16 * 1024 * 1024;
const MAX_GROUPS: u32 = 4096;
const MAX_ENTRIES: usize = 262_144;
const MAX_STRING: usize = 64 * 1024;

#[derive(Debug, Default)]
pub(crate) struct SentenceTable {
    groups: BTreeMap<String, BTreeMap<String, String>>,
}
impl SentenceTable {
    pub(crate) fn parse(bytes: &[u8]) -> Option<Self> {
        if bytes.len() > MAX_BYTES {
            return None;
        }
        struct Reader<'a> {
            b: &'a [u8],
            p: usize,
        }
        impl<'a> Reader<'a> {
            fn u32(&mut self) -> Option<u32> {
                let end = self.p.checked_add(4)?;
                let n = u32::from_le_bytes(self.b.get(self.p..end)?.try_into().ok()?);
                self.p = end;
                Some(n)
            }
            fn text(&mut self) -> Option<String> {
                let tail = self.b.get(self.p..)?;
                let end = tail.iter().take(MAX_STRING + 1).position(|b| *b == 0)?;
                let text = std::str::from_utf8(&tail[..end]).ok()?.to_owned();
                self.p = self.p.checked_add(end + 1)?;
                Some(text)
            }
        }
        let mut r = Reader { b: bytes, p: 0 };
        let count = r.u32()?;
        if count > MAX_GROUPS {
            return None;
        }
        let mut table = Self::default();
        let mut total = 0usize;
        for _ in 0..count {
            let group = r.text()?;
            let count = r.u32()? as usize;
            total = total.checked_add(count)?;
            if total > MAX_ENTRIES {
                return None;
            }
            let mut entries = BTreeMap::new();
            for _ in 0..count {
                let key = r.text()?;
                let text = r.text()?;
                entries.insert(key, text);
            }
            table.groups.insert(group, entries);
        }
        if r.p != bytes.len() {
            return None;
        }
        Some(table)
    }
    pub(crate) fn merge(&mut self, incoming: Self) {
        self.groups.extend(incoming.groups);
    }
    pub(crate) fn get(&self, group: &str, key: &str) -> Option<&str> {
        self.groups.get(group)?.get(key).map(String::as_str)
    }
    pub(crate) fn valid_size(length: usize) -> bool {
        length <= MAX_BYTES
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> Vec<u8> {
        let mut b = 1u32.to_le_bytes().to_vec();
        b.extend_from_slice(b"section\0");
        b.extend_from_slice(&2u32.to_le_bytes());
        for (k, v) in [("000000", "原创测试正文"), ("000001", "second")] {
            b.extend_from_slice(k.as_bytes());
            b.push(0);
            b.extend_from_slice(v.as_bytes());
            b.push(0);
        }
        b
    }
    #[test]
    fn parses_original_utf8_groups_without_global_numeric_replacement() {
        let b = fixture();
        let t = SentenceTable::parse(&b).unwrap();
        assert_eq!(t.get("section", "000000"), Some("原创测试正文"));
        assert_eq!(t.get("other", "000000"), None);
        assert_eq!(t.get("section", "missing"), None);
    }
    #[test]
    fn truncation_invalid_utf8_counts_and_trailing_bytes_are_rejected() {
        let b = fixture();
        for n in 0..b.len() {
            assert!(SentenceTable::parse(&b[..n]).is_none());
        }
        let mut bad = b.clone();
        bad.push(0);
        assert!(SentenceTable::parse(&bad).is_none());
        let mut bad = b.clone();
        bad[4] = 0xff;
        assert!(SentenceTable::parse(&bad).is_none());
        let mut bad = b.clone();
        bad[..4].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(SentenceTable::parse(&bad).is_none());
        assert!(!SentenceTable::valid_size(MAX_BYTES + 1));
    }
    #[test]
    fn duplicate_keys_keep_the_last_value_and_groups_replace_as_units() {
        let mut bytes = fixture();
        let at = bytes.windows(6).position(|part| part == b"000001").unwrap();
        bytes[at..at + 6].copy_from_slice(b"000000");
        let mut table = SentenceTable::parse(&bytes).unwrap();
        assert_eq!(table.get("section", "000000"), Some("second"));
        assert_eq!(table.get("section", "000001"), None);
        let mut empty = 1u32.to_le_bytes().to_vec();
        empty.extend_from_slice(b"section\0");
        empty.extend_from_slice(&0u32.to_le_bytes());
        table.merge(SentenceTable::parse(&empty).unwrap());
        assert_eq!(table.get("section", "000000"), None);
        let mut duplicate = fixture();
        duplicate[..4].copy_from_slice(&2u32.to_le_bytes());
        duplicate.extend_from_slice(&empty[4..]);
        assert_eq!(
            SentenceTable::parse(&duplicate)
                .unwrap()
                .get("section", "000000"),
            None
        );
    }

    #[test]
    fn arbitrary_small_inputs_do_not_panic() {
        let mut seed = 1u32;
        for len in 0..512 {
            let mut b = vec![0; len];
            for v in &mut b {
                seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                *v = (seed >> 24) as u8;
            }
            let _ = SentenceTable::parse(&b);
        }
    }
}
