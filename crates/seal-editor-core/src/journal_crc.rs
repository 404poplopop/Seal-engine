use std::fs;
use std::path::Path;

use crate::error::CoreError;

/// One journal record. `kind` is owned by the writer; the shell uses 1 for an edit snapshot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JournalRecord {
    pub kind: u8,
    pub seq: u64,
    pub payload: Vec<u8>,
}

/// Append a length-prefixed record and its CRC32C. A short or corrupt tail is ignored on read.
pub fn append_record(path: &Path, record: &JournalRecord) -> Result<(), CoreError> {
    let bytes = encode(record);
    crate::fault_io::append_bytes(path, &bytes, &crate::fault_io::FaultPlan::none())
}

/// Same as [`append_record`], but applies a test [`FaultPlan`] on the write path.
pub fn append_record_fault(
    path: &Path,
    record: &JournalRecord,
    plan: &crate::fault_io::FaultPlan,
) -> Result<(), CoreError> {
    let bytes = encode(record);
    crate::fault_io::append_bytes(path, &bytes, plan)
}

/// Replace the file with these records. A crash keeps the previous file.
pub fn write_records(path: &Path, records: &[JournalRecord]) -> Result<(), CoreError> {
    let mut bytes = Vec::new();
    for record in records {
        bytes.extend(encode(record));
    }
    crate::persist::atomic_write(path, &bytes)
}

/// Drop every record after a save that has reached disk.
pub fn clear_records(path: &Path) -> Result<(), CoreError> {
    if path.exists() {
        crate::persist::atomic_write(path, &[])?;
    }
    Ok(())
}

pub fn read_records(path: &Path) -> Result<Vec<JournalRecord>, CoreError> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let bytes = fs::read(path)?;
    Ok(decode_complete(&bytes))
}

pub fn decode_complete(bytes: &[u8]) -> Vec<JournalRecord> {
    let mut out = Vec::new();
    let mut offset = 0;
    while offset + 4 <= bytes.len() {
        let len =
            u32::from_le_bytes(bytes[offset..offset + 4].try_into().expect("4 bytes")) as usize;
        let body_end = offset + 4 + len;
        let crc_end = body_end + 4;
        if crc_end > bytes.len() {
            break;
        }
        let body = &bytes[offset + 4..body_end];
        let crc = u32::from_le_bytes(bytes[body_end..crc_end].try_into().expect("4 bytes"));
        if crc32c(body) == crc {
            if let Some(record) = decode_body(body) {
                out.push(record);
            }
        }
        offset = crc_end;
    }
    out
}

fn encode(record: &JournalRecord) -> Vec<u8> {
    let mut body = Vec::with_capacity(1 + 8 + record.payload.len());
    body.push(record.kind);
    body.extend_from_slice(&record.seq.to_le_bytes());
    body.extend_from_slice(&record.payload);
    let mut bytes = Vec::with_capacity(4 + body.len() + 4);
    bytes.extend_from_slice(&(body.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&body);
    bytes.extend_from_slice(&crc32c(&body).to_le_bytes());
    bytes
}

fn decode_body(body: &[u8]) -> Option<JournalRecord> {
    if body.len() < 9 {
        return None;
    }
    let kind = body[0];
    let seq = u64::from_le_bytes(body[1..9].try_into().ok()?);
    Some(JournalRecord { kind, seq, payload: body[9..].to_vec() })
}

/// Castagnoli CRC32C, the checksum on each journal record.
pub fn crc32c(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &byte in data {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0x82F6_3B78 & mask);
        }
    }
    !crc
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checksum_matches_the_known_vector() {
        assert_eq!(crc32c(b"123456789"), 0xE306_9283);
    }

    #[test]
    fn torn_tail_and_bad_checksum_are_dropped() {
        let record = JournalRecord { kind: 1, seq: 4, payload: b"keep".to_vec() };
        let mut bytes = encode(&record);
        let mut torn = bytes.clone();
        torn.extend_from_slice(&[1, 2, 3]);
        assert_eq!(decode_complete(&torn), vec![record.clone()]);
        let crc_at = bytes.len() - 1;
        bytes[crc_at] ^= 0xff;
        assert!(decode_complete(&bytes).is_empty());
    }

    #[test]
    fn a_cut_at_two_hundred_points_keeps_only_whole_records() {
        let mut bytes = Vec::new();
        for seq in 0..8u64 {
            let record = JournalRecord { kind: 1, seq, payload: vec![seq as u8; 24] };
            bytes.extend(encode(&record));
        }
        let whole = decode_complete(&bytes);
        let mut state = 0x1234_5678u64;
        for _ in 0..200 {
            state = state.wrapping_mul(1664525).wrapping_add(1013904223);
            let cut = (state as usize) % (bytes.len() + 1);
            let decoded = decode_complete(&bytes[..cut]);
            assert!(decoded.len() <= whole.len());
            assert_eq!(&whole[..decoded.len()], decoded.as_slice());
        }
    }
}
