//! UUIDv7: a 48-bit big-endian millisecond timestamp, then 74 bits of randomness.
//!
//! The client mints every id, which is what lets a record created with the radio off reach the
//! server without ever being remapped. Being time-ordered also means a plain sort by id is a
//! sort by creation time, so no separate index is needed for "newest first".

/// The instant and the randomness both come from the caller: this crate has neither a clock nor
/// an entropy source, and a shell that has to supply them cannot forget that ids are minted
/// locally.
pub fn uuidv7(epoch_ms: i64, random: [u8; 10]) -> String {
    let mut bytes = [0_u8; 16];

    bytes[..6].copy_from_slice(&(epoch_ms as u64).to_be_bytes()[2..]);
    bytes[6..].copy_from_slice(&random);
    bytes[6] = (bytes[6] & 0x0f) | 0x70; // version 7
    bytes[8] = (bytes[8] & 0x3f) | 0x80; // RFC 4122 variant

    let hex: String = bytes.iter().map(|byte| format!("{byte:02x}")).collect();

    format!(
        "{}-{}-{}-{}-{}",
        &hex[..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..]
    )
}

/// The millisecond an id was minted at, or `None` if the text is not a UUIDv7.
pub fn minted_at(id: &str) -> Option<i64> {
    if !is_uuid(id) || id.as_bytes()[14] != b'7' {
        return None;
    }

    i64::from_str_radix(&format!("{}{}", &id[..8], &id[9..13]), 16).ok()
}

/// Shape only: 8-4-4-4-12 lower-case hex. Nothing here says the id is one of ours.
pub fn is_uuid(id: &str) -> bool {
    let groups = [8, 4, 4, 4, 12];
    let mut parts = id.split('-');

    groups.iter().all(|length| {
        parts.next().is_some_and(|part| {
            part.len() == *length
                && part
                    .bytes()
                    .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        })
    }) && parts.next().is_none()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_the_shape_every_id_column_expects() {
        let id = uuidv7(1_757_332_200_123, [0xab; 10]);

        assert!(is_uuid(&id), "{id}");
        assert_eq!(id.as_bytes()[14], b'7', "version 7");
        assert!(
            matches!(id.as_bytes()[19], b'8' | b'9' | b'a' | b'b'),
            "RFC 4122 variant"
        );
    }

    /// The property the whole scheme rests on: sorting ids sorts by creation time, across
    /// devices that have never spoken to each other.
    #[test]
    fn sorts_by_the_moment_it_was_minted() {
        let mut ids: Vec<String> = [3_000_000_000_000, 1, 1_757_332_200_123]
            .iter()
            .map(|instant| uuidv7(*instant, [0x00; 10]))
            .collect();
        ids.sort();

        assert_eq!(
            ids,
            [
                uuidv7(1, [0x00; 10]),
                uuidv7(1_757_332_200_123, [0x00; 10]),
                uuidv7(3_000_000_000_000, [0x00; 10])
            ]
        );
    }

    #[test]
    fn two_ids_from_the_same_millisecond_still_differ() {
        assert_ne!(
            uuidv7(1_757_332_200_123, [0x01; 10]),
            uuidv7(1_757_332_200_123, [0x02; 10])
        );
    }

    #[test]
    fn reads_back_the_moment_it_carries() {
        let instant = 1_757_332_200_123;

        assert_eq!(minted_at(&uuidv7(instant, [0x7f; 10])), Some(instant));
        assert_eq!(minted_at("not an id"), None);
        // A v4 id carries no time, and must not be read as though it did.
        assert_eq!(minted_at("9b1deb4d-3b7d-4bad-9bdd-2b0d7b3dcb6d"), None);
    }

    #[test]
    fn recognises_the_shape_and_nothing_else() {
        assert!(is_uuid("0198f2b1-c0bb-7abb-abab-ababababab00"));
        assert!(
            !is_uuid("0198F2B1-C0BB-7ABB-ABAB-ABABABABAB00"),
            "upper case"
        );
        assert!(!is_uuid("0198f2b1c0bb7abbababababababab00"), "no dashes");
        assert!(!is_uuid("0198f2b1-c0bb-7abb-abab-ababababab00-extra"));
        assert!(!is_uuid(""));
    }
}
