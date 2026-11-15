//! Charset selection and packed C-string output for native font enumeration.
//! The caller supplies one entry per available family, with its actual charset
//! capabilities, and encodes names using the active guest text encoding.

pub(crate) struct NativeFontFamily {
    pub(crate) name: Vec<u8>,
    pub(crate) charsets: Vec<u8>,
}

/// Returns the packed names and the number of families. A null destination
/// queries `bytes.len()`; a nonnull destination receives those bytes and returns
/// the count. There is one terminator per name and no extra list terminator.
pub(crate) fn serialize_native_records(
    records: &[NativeFontFamily],
    charset: i32,
) -> (Vec<u8>, usize) {
    // LOGFONT.lfCharSet is a BYTE. DEFAULT_CHARSET enumerates all families;
    // every other value selects that actual charset, including 255 for -1.
    let charset = charset as u8;
    let mut bytes = Vec::new();
    let mut count = 0;
    for record in records {
        if charset != 1 && !record.charsets.contains(&charset) {
            continue;
        }
        let end = record
            .name
            .iter()
            .position(|&byte| byte == 0)
            .unwrap_or(record.name.len());
        bytes.extend_from_slice(&record.name[..end]);
        bytes.push(0);
        count += 1;
    }
    (bytes, count)
}

#[cfg(test)]
mod tests {
    use super::{NativeFontFamily, serialize_native_records};

    fn family(name: &[u8], charsets: &[u8]) -> NativeFontFamily {
        NativeFontFamily {
            name: name.to_vec(),
            charsets: charsets.to_vec(),
        }
    }

    #[test]
    fn a_specific_charset_selects_only_declared_family_capabilities() {
        let families = [
            family(b"First", &[0, 128]),
            family(b"Second", &[128, 134]),
            family(b"Third", &[136]),
        ];
        assert_eq!(
            serialize_native_records(&families, 0),
            (b"First\0".to_vec(), 1)
        );
        assert_eq!(
            serialize_native_records(&families, 128),
            (b"First\0Second\0".to_vec(), 2)
        );
        assert_eq!(
            serialize_native_records(&families, 134),
            (b"Second\0".to_vec(), 1)
        );
        assert_eq!(
            serialize_native_records(&families, 136),
            (b"Third\0".to_vec(), 1)
        );
        assert_eq!(serialize_native_records(&families, 2), (Vec::new(), 0));
    }

    #[test]
    fn default_charset_includes_all_available_families_in_catalog_order() {
        let families = [family(b"Only CJK", &[128]), family(b"Unclassified", &[])];
        assert_eq!(
            serialize_native_records(&families, 1),
            (b"Only CJK\0Unclassified\0".to_vec(), 2)
        );
    }

    #[test]
    fn the_native_byte_cast_precedes_default_or_specific_selection() {
        let families = [family(b"CJK", &[128]), family(b"OEM", &[255])];
        for charset in [128, 0x180, -128] {
            assert_eq!(
                serialize_native_records(&families, charset),
                (b"CJK\0".to_vec(), 1)
            );
        }
        assert_eq!(
            serialize_native_records(&families, -1),
            (b"OEM\0".to_vec(), 1)
        );
        assert_eq!(
            serialize_native_records(&families, 257),
            (b"CJK\0OEM\0".to_vec(), 2)
        );
    }

    #[test]
    fn byte_query_and_write_count_use_encoded_c_strings_without_record_padding() {
        let families = [
            family("字体A".as_bytes(), &[128]),
            family(b"Second", &[128]),
        ];
        let (bytes, count) = serialize_native_records(&families, 128);
        assert_eq!(bytes, "字体A\0Second\0".as_bytes());
        assert_eq!(bytes.len(), 15);
        assert_eq!(count, 2);
    }

    #[test]
    fn each_name_stops_at_its_first_nul_and_gets_exactly_one_terminator() {
        let families = [family(b"First\0hidden\0", &[0]), family(b"Second\0", &[0])];
        assert_eq!(
            serialize_native_records(&families, 0),
            (b"First\0Second\0".to_vec(), 2)
        );
    }

    #[test]
    fn an_empty_catalog_or_no_match_has_no_bytes_and_no_list_terminator() {
        assert_eq!(serialize_native_records(&[], 1), (Vec::new(), 0));
        assert_eq!(
            serialize_native_records(&[family(b"A", &[128])], 0),
            (Vec::new(), 0)
        );
    }

    #[test]
    fn serialization_preserves_callback_order_and_count_without_name_filtering() {
        let families = [
            family(b"Same", &[128, 128]),
            family(b"", &[128]),
            family(b"Same", &[128]),
        ];
        assert_eq!(
            serialize_native_records(&families, 128),
            (b"Same\0\0Same\0".to_vec(), 3)
        );
    }
}
