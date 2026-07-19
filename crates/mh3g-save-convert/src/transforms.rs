#[cfg(test)]
mod tests {
    use crate::profile::PAYLOAD_SIZE;

    use super::apply_endian_swaps;

    #[test]
    fn transforms_endian_swaps_reverse_declared_spans_only() {
        let mut payload = vec![0_u8; PAYLOAD_SIZE];
        payload[0x20..0x24].copy_from_slice(&[1, 2, 3, 4]);
        payload[0x2A..0x2C].copy_from_slice(&[5, 6]);
        payload[0x10] = 0xAA;

        apply_endian_swaps(&mut payload).unwrap();

        assert_eq!(&payload[0x20..0x24], &[4, 3, 2, 1]);
        assert_eq!(&payload[0x2A..0x2C], &[6, 5]);
        assert_eq!(payload[0x10], 0xAA);
    }

    #[test]
    fn transforms_endian_swaps_reject_wrong_length_before_mutating() {
        let mut payload = vec![0x5A; PAYLOAD_SIZE - 1];
        let original = payload.clone();

        assert!(apply_endian_swaps(&mut payload).is_err());
        assert_eq!(payload, original);
    }
}
