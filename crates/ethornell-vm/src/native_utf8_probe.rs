//! The three-way native byte probe behind Sys81:27 (49FFC0 / 49E7F0).
//! This tests a terminated byte string, not Unicode validity: ASCII returns
//! bit 31, and only the first three multibyte sequences are examined.

pub(crate) fn classify(bytes: &[u8]) -> i32 {
    let mut offset = 0;
    let mut multibyte_sequences = 0;
    while let Some(&lead) = bytes.get(offset) {
        if lead == 0 {
            break;
        }
        if lead < 0x80 {
            offset += 1;
            continue;
        }
        // Native rejects continuation bytes and the C0/C1 leaders, but
        // places no Unicode scalar, overlong or surrogate restrictions on
        // the remaining leaders, including the old F8..FF widths.
        if lead < 0xc2 {
            return 0;
        }
        let mut width = 1;
        loop {
            width += 1;
            if u32::from(lead) & (0x80_u32 >> width) == 0 {
                break;
            }
        }
        let Some(sequence) = bytes.get(offset..).and_then(|tail| tail.get(..width)) else {
            return 0;
        };
        if sequence[1..].iter().any(|byte| byte & 0xc0 != 0x80) {
            return 0;
        }
        multibyte_sequences += 1;
        // Return before advancing to or inspecting any bytes after the
        // third valid multibyte sequence, as the native branch does.
        if multibyte_sequences == 3 {
            return 1;
        }
        offset += width;
    }
    if multibyte_sequences == 0 {
        i32::MIN
    } else {
        1
    }
}

#[cfg(test)]
mod tests {
    use super::classify;

    #[test]
    fn ascii_and_empty_strings_return_the_high_bit_until_the_first_nul() {
        for bytes in [
            &b""[..],
            &b"plain ASCII"[..],
            &[0x7f][..],
            &[0, 0xff][..],
            &b"a\0\xff"[..],
        ] {
            assert_eq!(classify(bytes), i32::MIN, "{bytes:x?}");
        }
    }

    #[test]
    fn lead_boundaries_include_the_native_extended_sequence_widths() {
        for (lead, width) in [
            (0xc2, 2),
            (0xdf, 2),
            (0xe0, 3),
            (0xef, 3),
            (0xf0, 4),
            (0xf7, 4),
            (0xf8, 5),
            (0xfb, 5),
            (0xfc, 6),
            (0xfd, 6),
            (0xfe, 7),
            (0xff, 8),
        ] {
            let mut bytes = vec![0x80; width];
            bytes[0] = lead;
            assert_eq!(classify(&bytes), 1, "{bytes:x?}");
            bytes[1..].fill(0xbf);
            assert_eq!(classify(&bytes), 1, "{bytes:x?}");
        }
        for lead in [0x80, 0xbf, 0xc0, 0xc1] {
            assert_eq!(classify(&[lead, 0x80, 0x80]), 0);
        }
    }

    #[test]
    fn every_required_continuation_must_exist_and_have_the_correct_prefix() {
        for (lead, width) in [
            (0xc2, 2),
            (0xe0, 3),
            (0xf0, 4),
            (0xf8, 5),
            (0xfc, 6),
            (0xfe, 7),
            (0xff, 8),
        ] {
            let mut bytes = vec![0x80; width];
            bytes[0] = lead;
            for end in 1..width {
                assert_eq!(classify(&bytes[..end]), 0, "truncated {bytes:x?} at {end}");
            }
            for position in 1..width {
                for invalid in [0, 0x7f, 0xc0, 0xff] {
                    bytes[position] = invalid;
                    assert_eq!(classify(&bytes), 0, "{bytes:x?}");
                }
                bytes[position] = 0x80;
            }
        }
    }

    #[test]
    fn it_stops_after_three_multibyte_sequences_not_three_characters() {
        let mut bytes = vec![0xc2, 0x80];
        bytes.extend_from_slice(b"ascii between sequences");
        bytes.extend_from_slice(&[0xc2, 0x80, 0xff]);
        assert_eq!(classify(&bytes), 0);
        let mut bytes = vec![0xc2, 0x80, 0xc2, 0x80, 0xc2, 0x80];
        bytes.extend_from_slice(&[0xff]);
        assert_eq!(classify(&bytes), 1);
        assert_eq!(classify(&[0xc2, 0x80, 0xc2, 0x80, 0xe2, 0x80, b'x']), 0);
        assert_eq!(classify(&[0xc2, 0x80, 0, 0xff]), 1);
    }

    #[test]
    fn the_native_probe_is_intentionally_different_from_strict_unicode_validation() {
        for bytes in [
            &[0xe0, 0x80, 0x80][..],
            &[0xed, 0xa0, 0x80][..],
            &[0xf8, 0x80, 0x80, 0x80, 0x80][..],
            &[0xc2, 0x80, 0xc2, 0x80, 0xc2, 0x80, 0xff][..],
        ] {
            assert!(std::str::from_utf8(bytes).is_err());
            assert_eq!(classify(bytes), 1, "{bytes:x?}");
        }
    }
}
