//! Bounded x86 API matching, independent of executable names and ABI profiles.
//!
//! This module has no app dependencies: `rustc --test` can exercise the actual
//! matcher with project-owned instruction buffers.

pub(crate) trait Image {
    fn code(&self, address: u64) -> Option<&[u8]>;
    fn data(&self, address: u64) -> Option<&[u8]>;
    fn writable(&self, address: u64, size: u32) -> bool;
}

fn at(bytes: &[u8], offset: usize, expected: &[u8]) -> bool {
    bytes.get(offset..offset + expected.len()) == Some(expected)
}

fn integer(bytes: &[u8], offset: usize) -> Option<u64> {
    Some(u64::from(u32::from_le_bytes(
        bytes.get(offset..offset + 4)?.try_into().ok()?,
    )))
}

fn relative(bytes: &[u8], address: u64, offset: usize, opcode: u8) -> Option<u64> {
    if bytes.get(offset) != Some(&opcode) {
        return None;
    }
    let displacement = i32::from_le_bytes(bytes.get(offset + 1..offset + 5)?.try_into().ok()?);
    address
        .checked_add(offset as u64 + 5)?
        .checked_add_signed(i64::from(displacement))
        .filter(|&target| target <= u64::from(u32::MAX))
}

fn body(image: &impl Image, mut address: u64) -> Option<(u64, &[u8])> {
    for _ in 0..4 {
        let bytes = image.code(address).filter(|s| !s.is_empty())?;
        if bytes[0] != 0xe9 {
            return Some((address, bytes));
        }
        address = relative(bytes, address, 0, 0xe9)?;
    }
    None
}

fn call(image: &impl Image, bytes: &[u8], address: u64, offset: usize) -> Option<u64> {
    let target = relative(bytes, address, offset, 0xe8)?;
    body(image, target)?;
    Some(target)
}

// Fetch a byte from VM code, narrow the selector, load its function pointer,
// check for null, then invoke it with the same VM. Only this 96-byte window is
// searched; multiple dispatch algorithms in that window are ambiguous.
fn dispatch(image: &impl Image, address: u64) -> Option<(u64, u64)> {
    let (address, bytes) = body(image, address)?;
    let mut candidates = bytes[..bytes.len().min(96)]
        .windows(24)
        .enumerate()
        .filter(|(_, s)| {
            at(s, 0, &[0x8b, 0xce, 0xe8])
                && at(s, 7, &[0x0f, 0xb6, 0xc8, 0x8b, 0x04, 0x8d])
                && at(s, 17, &[0x85, 0xc0, 0x74, 0x15, 0x56, 0xff, 0xd0])
        });
    let (offset, candidate) = candidates.next()?;
    if candidates.next().is_some() {
        return None;
    }
    let fetch = call(image, bytes, address, offset + 2)?;
    let (_, fetch_bytes) = body(image, fetch)?;
    if !fetch_bytes.starts_with(&[
        0x8b, 0x91, 0x80, 0, 0, 0, 0x8b, 0x41, 0x2c, 0x8a, 4, 2, 0x42, 0x89, 0x91, 0x80, 0, 0, 0,
        0xc3,
    ]) {
        return None;
    }
    Some((integer(candidate, 13)?, fetch))
}

fn slot(image: &impl Image, table: u64, index: usize) -> Option<u64> {
    integer(image.data(table)?.get(..1024)?, index * 4)
}

fn mode_global(image: &impl Image, handler: u64) -> Option<(u64, u64)> {
    let (address, bytes) = body(image, handler)?;
    if !at(bytes, 0, &[0x55, 0x8b, 0xec, 0x8b, 0x4d, 8])
        || !at(bytes, 11, &[0x8b, 0xc8])
        || !at(bytes, 18, &[0x8b, 0x4d, 8, 0x50])
        || !at(bytes, 27, &[0x33, 0xc0, 0x5d, 0xc3])
    {
        return None;
    }
    let pop = call(image, bytes, address, 6)?;
    let (_, pop_bytes) = body(image, pop)?;
    if !pop_bytes.starts_with(&[
        0x8b, 0x51, 0x78, 0x85, 0xd2, 0x75, 3, 0x8b, 0x51, 0x10, 0x8b, 0x41, 0x18, 0x4a, 0x89,
        0x51, 0x78, 0x8b, 4, 0x90, 0xc3,
    ]) {
        return None;
    }
    call(image, bytes, address, 22)?;
    let setter = call(image, bytes, address, 13)?;
    let (_, bytes) = body(image, setter)?;
    // Accepted modes store (flag, codepage) = (0,932) or (1,65001), returning
    // one. Invalid modes branch directly to return zero without either store.
    if !at(
        bytes,
        0,
        &[0x83, 0xe9, 0, 0x74, 0x22, 0x83, 0xe9, 1, 0x75, 0x1a],
    ) || !at(bytes, 36, &[0x33, 0xc0, 0xc3])
    {
        return None;
    }
    for (offset, flag, codepage) in [(10, 1, 65001u32), (39, 0, 932)] {
        if !at(bytes, offset, &[0xc7, 0x05])
            || integer(bytes, offset + 6)? != flag
            || !at(bytes, offset + 10, &[0xb8, 1, 0, 0, 0, 0xc7, 0x05])
            || integer(bytes, offset + 21)? != u64::from(codepage)
            || bytes.get(offset + 25) != Some(&0xc3)
        {
            return None;
        }
    }
    let flag = integer(bytes, 12)?;
    let codepage = integer(bytes, 27)?;
    (integer(bytes, 41)? == flag
        && integer(bytes, 56)? == codepage
        && flag != codepage
        && image.writable(flag, 4)
        && image.writable(codepage, 4))
    .then_some((flag, pop))
}

fn glyph_call(image: &impl Image, handler: u64, pop: u64) -> Option<u64> {
    let (address, bytes) = body(image, handler)?;
    let parts: &[(usize, &[u8])] = &[
        (0, &[0x55, 0x8b, 0xec, 0x81, 0xec, 0x14, 1, 0, 0, 0xa1]),
        (
            14,
            &[
                0x33, 0xc5, 0x89, 0x45, 0xfc, 0x53, 0x8b, 0x5d, 8, 0x8b, 0xcb, 0x56, 0x57,
            ],
        ),
        (32, &[0x89, 0x85, 0xf8, 0xfe, 0xff, 0xff]),
        (43, &[0x89, 0x85, 0xf4, 0xfe, 0xff, 0xff]),
        (54, &[0x8b, 0xf8]),
        (61, &[0x8b, 0xf0]),
        (68, &[0x89, 0x85, 0xf0, 0xfe, 0xff, 0xff]),
        (
            79,
            &[
                0xff, 0xb5, 0xf8, 0xfe, 0xff, 0xff, 0x8b, 0xc8, 0x89, 0x85, 0xec, 0xfe, 0xff, 0xff,
                0xff, 0xb5, 0xf4, 0xfe, 0xff, 0xff, 0x57, 0x56, 0x8b, 0xb5, 0xf0, 0xfe, 0xff, 0xff,
                0x8b, 0xd6,
            ],
        ),
        (114, &[0x83, 0xc4, 0x10]),
    ];
    if !parts
        .iter()
        .all(|&(offset, pattern)| at(bytes, offset, pattern))
        || !image.writable(integer(bytes, 10)?, 4)
    {
        return None;
    }
    for offset in [27, 38, 49, 56, 63, 74] {
        if call(image, bytes, address, offset)? != pop {
            return None;
        }
    }
    // The six-operand wrapper's call lies beyond the older 96-byte call scan.
    call(image, bytes, address, 109)
}

fn glyph_implementation(image: &impl Image, implementation: u64) -> Option<()> {
    let (address, bytes) = body(image, implementation)?;
    if !at(
        bytes,
        0,
        &[
            0x55, 0x8b, 0xec, 0x83, 0xe4, 0xf8, 0x83, 0xec, 0x18, 0x53, 0x56, 0x8b, 0xd9,
        ],
    ) || !at(bytes, 18, &[0x85, 0xc0, 0x0f, 0x84])
        || !at(bytes, 26, &[0x0f, 0xb6, 0xf3])
    {
        return None;
    }
    let validator = call(image, bytes, address, 13)?;
    let (_, bytes) = body(image, validator)?;
    // Tagged F001..F7FF and untagged FF01..FFFF; the caller uses the low byte
    // as its image slot. These ranges alone do not imply an automatic bridge.
    bytes
        .starts_with(&[
            0x85, 0xc9, 0x79, 0x17, 0x81, 0xe1, 0xff, 0xff, 0xff, 0x7f, 0x8d, 0x81, 0xff, 0x0f,
            0xff, 0xff, 0xb9, 0xfe, 7, 0, 0, 0x3b, 0xc8, 0x1b, 0xc0, 0x40, 0xc3, 0x8d, 0x81, 0xff,
            0, 0xff, 0xff, 0xb9, 0xfe, 0, 0, 0, 0x3b, 0xc8, 0x1b, 0xc0, 0x40, 0xc3,
        ])
        .then_some(())
}

fn chain(image: &impl Image, table: &[u64]) -> Option<[u64; 6]> {
    if table.len() != 256 {
        return None;
    }
    let (system, fetch) = dispatch(image, table[0x81])?;
    let (graphics, graphics_fetch) = dispatch(image, table[0x92])?;
    if fetch != graphics_fetch {
        return None;
    }
    let (flag, pop) = mode_global(image, slot(image, system, 0)?)?;
    let bridge = glyph_call(image, slot(image, graphics, 0x98)?, pop)?;
    let (address, bytes) = body(image, bridge)?;
    if !at(
        bytes,
        0,
        &[
            0x55, 0x8b, 0xec, 0xff, 0x75, 0x14, 0xff, 0x75, 0x10, 0xff, 0x75, 0x0c, 0xff, 0x75, 8,
        ],
    ) || !at(
        bytes,
        20,
        &[0xf7, 0xd8, 0x1b, 0xc0, 0x25, 0, 0, 0, 0x80, 0x0b, 0xc8],
    ) || !at(bytes, 36, &[0x83, 0xc4, 0x10, 0x5d, 0xc3])
    {
        return None;
    }
    let getter = call(image, bytes, address, 15)?;
    let implementation = call(image, bytes, address, 31)?;
    let (_, getter_bytes) = body(image, getter)?;
    if !at(getter_bytes, 0, &[0x33, 0xc0, 0x83, 0x3d])
        || integer(getter_bytes, 4)? != flag
        || !at(getter_bytes, 8, &[1, 0x0f, 0x94, 0xc0, 0xc3])
    {
        return None;
    }
    glyph_implementation(image, implementation)?;
    Some([
        table[0x81],
        table[0x92],
        flag,
        bridge,
        getter,
        implementation,
    ])
}

pub(crate) fn detect(image: &impl Image, tables: &[Vec<u64>]) -> bool {
    if tables.is_empty() || tables.len() > 8 {
        return false;
    }
    let Some(first) = chain(image, &tables[0]) else {
        return false;
    };
    tables
        .iter()
        .skip(1)
        .all(|table| chain(image, table) == Some(first))
}

#[cfg(test)]
mod tests {
    use super::*;

    const POPS: [usize; 6] = [27, 38, 49, 56, 63, 74];

    struct Segment {
        address: u64,
        bytes: Vec<u8>,
        executable: bool,
        writable: bool,
    }

    struct Memory(Vec<Segment>);

    impl Memory {
        fn insert(&mut self, address: u64, bytes: Vec<u8>, executable: bool, writable: bool) {
            self.0.push(Segment {
                address,
                bytes,
                executable,
                writable,
            });
        }

        fn bytes_mut(&mut self, address: u64) -> &mut Vec<u8> {
            &mut self
                .0
                .iter_mut()
                .find(|s| s.address == address)
                .unwrap()
                .bytes
        }

        fn at(&self, address: u64, executable: bool) -> Option<&[u8]> {
            self.0.iter().find_map(|s| {
                let offset = usize::try_from(address.checked_sub(s.address)?).ok()?;
                (s.executable == executable && offset < s.bytes.len())
                    .then(|| s.bytes.get(offset..))
                    .flatten()
            })
        }
    }

    impl Image for Memory {
        fn code(&self, address: u64) -> Option<&[u8]> {
            self.at(address, true)
        }
        fn data(&self, address: u64) -> Option<&[u8]> {
            self.at(address, false)
        }
        fn writable(&self, address: u64, size: u32) -> bool {
            self.0.iter().any(|s| {
                s.writable
                    && address.checked_sub(s.address).is_some_and(|offset| {
                        offset
                            .checked_add(u64::from(size))
                            .is_some_and(|end| end <= s.bytes.len() as u64)
                    })
            })
        }
    }

    fn put(bytes: &mut [u8], offset: usize, value: &[u8]) {
        bytes[offset..offset + value.len()].copy_from_slice(value);
    }

    fn integer(bytes: &mut [u8], offset: usize, value: u64) {
        put(bytes, offset, &(value as u32).to_le_bytes());
    }

    fn call(bytes: &mut [u8], offset: usize, source: u64, target: u64) {
        bytes[offset] = 0xe8;
        integer(
            bytes,
            offset + 1,
            target.wrapping_sub(source + offset as u64 + 5),
        );
    }

    struct Fixture {
        memory: Memory,
        main: Vec<u64>,
        base: u64,
    }

    impl Fixture {
        fn new(base: u64) -> Self {
            let mut memory = Memory(Vec::new());
            let addr = |offset| base + offset;
            // Code-byte fetch and operand-ring pop are separate VM contracts.
            for (offset, table) in [(0x100, 0x2000), (0x200, 0x2400)] {
                let mut bytes = vec![0x90; 28];
                put(&mut bytes, 4, &[0x8b, 0xce]);
                call(&mut bytes, 6, addr(offset), addr(0xa00));
                put(&mut bytes, 11, &[0x0f, 0xb6, 0xc8, 0x8b, 0x04, 0x8d]);
                integer(&mut bytes, 17, addr(table));
                put(&mut bytes, 21, &[0x85, 0xc0, 0x74, 0x15, 0x56, 0xff, 0xd0]);
                memory.insert(addr(offset), bytes, true, false);
            }
            let mut mode = vec![0; 31];
            put(&mut mode, 0, &[0x55, 0x8b, 0xec, 0x8b, 0x4d, 0x08]);
            call(&mut mode, 6, addr(0x300), addr(0xb00));
            put(&mut mode, 11, &[0x8b, 0xc8]);
            call(&mut mode, 13, addr(0x300), addr(0x400));
            put(&mut mode, 18, &[0x8b, 0x4d, 0x08, 0x50]);
            call(&mut mode, 22, addr(0x300), addr(0xc00));
            put(&mut mode, 27, &[0x33, 0xc0, 0x5d, 0xc3]);
            memory.insert(addr(0x300), mode, true, false);

            let mut setter = vec![0; 65];
            put(
                &mut setter,
                0,
                &[0x83, 0xe9, 0, 0x74, 0x22, 0x83, 0xe9, 1, 0x75, 0x1a],
            );
            for (offset, value, encoding) in [(10, 1, 65001), (39, 0, 932)] {
                put(&mut setter, offset, &[0xc7, 0x05]);
                integer(&mut setter, offset + 2, addr(0x3000));
                integer(&mut setter, offset + 6, value);
                put(&mut setter, offset + 10, &[0xb8, 1, 0, 0, 0, 0xc7, 0x05]);
                integer(&mut setter, offset + 17, addr(0x3004));
                integer(&mut setter, offset + 21, encoding);
                setter[offset + 25] = 0xc3;
            }
            put(&mut setter, 36, &[0x33, 0xc0, 0xc3]);
            memory.insert(addr(0x400), setter, true, false);

            let mut wrapper = vec![0; 117];
            put(
                &mut wrapper,
                0,
                &[0x55, 0x8b, 0xec, 0x81, 0xec, 0x14, 1, 0, 0, 0xa1],
            );
            integer(&mut wrapper, 10, addr(0x3008));
            put(
                &mut wrapper,
                14,
                &[
                    0x33, 0xc5, 0x89, 0x45, 0xfc, 0x53, 0x8b, 0x5d, 8, 0x8b, 0xcb, 0x56, 0x57,
                ],
            );
            for offset in POPS {
                call(&mut wrapper, offset, addr(0x500), addr(0xb00));
            }
            put(&mut wrapper, 32, &[0x89, 0x85, 0xf8, 0xfe, 0xff, 0xff]);
            put(&mut wrapper, 43, &[0x89, 0x85, 0xf4, 0xfe, 0xff, 0xff]);
            put(&mut wrapper, 54, &[0x8b, 0xf8]);
            put(&mut wrapper, 61, &[0x8b, 0xf0]);
            put(&mut wrapper, 68, &[0x89, 0x85, 0xf0, 0xfe, 0xff, 0xff]);
            put(
                &mut wrapper,
                79,
                &[
                    0xff, 0xb5, 0xf8, 0xfe, 0xff, 0xff, 0x8b, 0xc8, 0x89, 0x85, 0xec, 0xfe, 0xff,
                    0xff, 0xff, 0xb5, 0xf4, 0xfe, 0xff, 0xff, 0x57, 0x56, 0x8b, 0xb5, 0xf0, 0xfe,
                    0xff, 0xff, 0x8b, 0xd6,
                ],
            );
            call(&mut wrapper, 109, addr(0x500), addr(0x600));
            put(&mut wrapper, 114, &[0x83, 0xc4, 0x10]);
            memory.insert(addr(0x500), wrapper, true, false);

            let mut bridge = vec![0; 41];
            put(
                &mut bridge,
                0,
                &[
                    0x55, 0x8b, 0xec, 0xff, 0x75, 0x14, 0xff, 0x75, 0x10, 0xff, 0x75, 0x0c, 0xff,
                    0x75, 8,
                ],
            );
            call(&mut bridge, 15, addr(0x600), addr(0x700));
            put(
                &mut bridge,
                20,
                &[0xf7, 0xd8, 0x1b, 0xc0, 0x25, 0, 0, 0, 0x80, 0x0b, 0xc8],
            );
            call(&mut bridge, 31, addr(0x600), addr(0x800));
            put(&mut bridge, 36, &[0x83, 0xc4, 0x10, 0x5d, 0xc3]);
            memory.insert(addr(0x600), bridge, true, false);
            let mut getter = vec![
                0x33, 0xc0, 0x83, 0x3d, 0, 0, 0, 0, 1, 0x0f, 0x94, 0xc0, 0xc3,
            ];
            integer(&mut getter, 4, addr(0x3000));
            memory.insert(addr(0x700), getter, true, false);

            let mut implementation = vec![0; 29];
            put(
                &mut implementation,
                0,
                &[
                    0x55, 0x8b, 0xec, 0x83, 0xe4, 0xf8, 0x83, 0xec, 0x18, 0x53, 0x56, 0x8b, 0xd9,
                ],
            );
            call(&mut implementation, 13, addr(0x800), addr(0x900));
            put(&mut implementation, 18, &[0x85, 0xc0, 0x0f, 0x84]);
            put(&mut implementation, 26, &[0x0f, 0xb6, 0xf3]);
            memory.insert(addr(0x800), implementation, true, false);
            memory.insert(
                addr(0x900),
                vec![
                    0x85, 0xc9, 0x79, 0x17, 0x81, 0xe1, 0xff, 0xff, 0xff, 0x7f, 0x8d, 0x81, 0xff,
                    0x0f, 0xff, 0xff, 0xb9, 0xfe, 7, 0, 0, 0x3b, 0xc8, 0x1b, 0xc0, 0x40, 0xc3,
                    0x8d, 0x81, 0xff, 0, 0xff, 0xff, 0xb9, 0xfe, 0, 0, 0, 0x3b, 0xc8, 0x1b, 0xc0,
                    0x40, 0xc3,
                ],
                true,
                false,
            );
            memory.insert(
                addr(0xa00),
                vec![
                    0x8b, 0x91, 0x80, 0, 0, 0, 0x8b, 0x41, 0x2c, 0x8a, 4, 2, 0x42, 0x89, 0x91,
                    0x80, 0, 0, 0, 0xc3,
                ],
                true,
                false,
            );
            memory.insert(
                addr(0xb00),
                vec![
                    0x8b, 0x51, 0x78, 0x85, 0xd2, 0x75, 3, 0x8b, 0x51, 0x10, 0x8b, 0x41, 0x18,
                    0x4a, 0x89, 0x51, 0x78, 0x8b, 4, 0x90, 0xc3,
                ],
                true,
                false,
            );
            memory.insert(addr(0xc00), vec![0xc3], true, false);
            for (offset, slot, target) in [(0x2000, 0, 0x300), (0x2400, 0x98, 0x500)] {
                let mut table = vec![0; 1024];
                integer(&mut table, slot * 4, addr(target));
                memory.insert(addr(offset), table, false, false);
            }
            memory.insert(addr(0x3000), vec![0; 12], false, true);
            let mut main = vec![0; 256];
            main[0x81] = addr(0x100);
            main[0x92] = addr(0x200);
            Self { memory, main, base }
        }

        fn detects(&self) -> bool {
            detect(&self.memory, std::slice::from_ref(&self.main))
        }
        fn bytes(&mut self, offset: u64) -> &mut Vec<u8> {
            self.memory.bytes_mut(self.base + offset)
        }
    }

    #[test]
    fn recognizes_relocated_independent_api_algorithms() {
        for base in [0x1000, 0x400000, 0x58000000] {
            assert!(Fixture::new(base).detects(), "base {base:x}");
        }
    }

    #[test]
    fn direct_forwarding_does_not_gain_a_unicode_bridge() {
        let mut f = Fixture::new(0x1000);
        let mut forward = vec![0x55, 0x8b, 0xec, 0x5d, 0xe9, 0, 0, 0, 0];
        integer(&mut forward, 5, 0x800 - 0x600 - 9);
        *f.bytes(0x600) = forward;
        assert!(!f.detects());
    }

    #[test]
    fn requires_the_same_writable_mode_global() {
        let mut f = Fixture::new(0x1000);
        integer(f.bytes(0x700), 4, 0x4004);
        assert!(!f.detects());
        let mut f = Fixture::new(0x1000);
        f.memory.0.last_mut().unwrap().writable = false;
        assert!(!f.detects());
    }

    #[test]
    fn rejects_changed_tag_masks_getter_conditions_and_encoding_modes() {
        for (function, offset, replacement) in [
            (0x600, 28, 0x40),
            (0x600, 29, 0x33),
            (0x700, 8, 0),
            (0x700, 10, 0x95),
            (0x400, 31, 0),
            (0x400, 8, 0x74),
        ] {
            let mut f = Fixture::new(0x1000);
            f.bytes(function)[offset] = replacement;
            assert!(!f.detects(), "function {function:x} byte {offset}");
        }
    }

    #[test]
    fn requires_all_six_pops_native_ranges_and_low_byte_slots() {
        let mut f = Fixture::new(0x1000);
        call(f.bytes(0x500), POPS[5], 0x1500, 0x1c00);
        assert!(!f.detects());
        for (function, offset) in [(0x900, 18), (0x900, 34), (0x800, 28)] {
            let mut f = Fixture::new(0x1000);
            f.bytes(function)[offset] ^= 1;
            assert!(!f.detects());
        }
    }

    #[test]
    fn rejects_every_truncated_required_buffer() {
        let f = Fixture::new(0x1000);
        for segment in &f.memory.0 {
            for length in 0..segment.bytes.len() {
                let mut truncated = Fixture::new(0x1000);
                truncated.memory.bytes_mut(segment.address).truncate(length);
                assert!(
                    !truncated.detects(),
                    "segment {:x} length {length}",
                    segment.address
                );
            }
        }
    }

    #[test]
    fn rejects_stale_targets_and_disagreeing_main_tables() {
        let mut f = Fixture::new(0x1000);
        call(f.bytes(0x600), 31, 0x1600, 0xdeadbeef);
        assert!(!f.detects());
        let f = Fixture::new(0x1000);
        let mut other = f.main.clone();
        other[0x92] = 0xdeadbeef;
        assert!(!detect(&f.memory, &[f.main, other]));
        assert!(!detect(&f.memory, &[]));
    }

    #[test]
    fn rejects_ambiguous_dispatchers_and_excessive_candidates() {
        let mut f = Fixture::new(0x1000);
        let duplicate = f.bytes(0x100)[4..28].to_vec();
        f.bytes(0x100).resize(64, 0x90);
        put(f.bytes(0x100), 32, &duplicate);
        assert!(!f.detects());
        let f = Fixture::new(0x1000);
        assert!(!detect(&f.memory, &vec![f.main; 9]));
    }
}
