use std::{
    collections::BTreeSet,
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::{
    ConversionError, io_at_path,
    process_probe::{PlatformProcessProbe, ProcessProbe},
    profile::build_jp_cemu_header,
    revision::ConverterRevision,
    transaction::{
        atomic_replace, remove_if_regular_file, sha256_hex, sync_directory, unique_path,
        write_new_file,
    },
    transforms::{
        GUILD_CARD_SLOT_SIZE, apply_japanese_wiiu_guild_card_slot_corrections,
        apply_japanese_wiiu_guild_card_slot_corrections_for_revision,
    },
};

const BOX_INFO_SIZE: usize = 0x20;
const MESSAGE_HEADER_SIZE: usize = 0x70;
pub const CEMU_HEADER_SIZE: usize = 40;
pub const CEMU_CEC_PAYLOAD_SIZE: usize = 0x835FC;
pub const CEMU_RECORD_AREA_OFFSET: usize = 0x1FC;
pub const CEMU_RECORD_SLOT_SIZE: usize = 0x2A00;
pub const CEMU_RECORD_SLOT_COUNT: usize = 50;
const CEC_GUILD_CARD_SLOT_COUNT: usize = CEMU_RECORD_SLOT_SIZE / GUILD_CARD_SLOT_SIZE;
const CEC_SOURCE_RECORD_PREFIX_SIZE: usize = 8;
const MH3G_TITLE_ID: u32 = 0x0004_8100;
const MH3G_BODY_SIZE: usize = CEC_SOURCE_RECORD_PREFIX_SIZE + CEMU_RECORD_SLOT_SIZE;
const MH3G_GUILD_CARD_OFFSET: usize = 0x7A04;
const GUILD_CARD_ANCHOR_SIZE: usize = 32;

#[derive(Debug, Serialize)]
pub struct CecBoxReport {
    pub path: PathBuf,
    pub size: usize,
    pub magic: Option<String>,
    pub box_info_size: Option<u32>,
    pub max_box_size: Option<u32>,
    pub box_size: Option<u32>,
    pub max_message_count: Option<u32>,
    pub declared_message_count: Option<u32>,
    pub max_batch_size: Option<u32>,
    pub max_message_size: Option<u32>,
    pub actual_message_count: usize,
    pub message_files: Vec<String>,
    pub header_valid: bool,
}

#[derive(Debug, Serialize)]
pub struct CecMessageReport {
    pub box_name: String,
    pub file: String,
    pub size: usize,
    pub sha256: String,
    pub header_valid: bool,
    pub magic: Option<String>,
    pub message_size: Option<u32>,
    pub header_size: Option<u32>,
    pub body_size: Option<u32>,
    pub title_id: Option<String>,
    pub title_id2: Option<String>,
    pub batch_id: Option<u32>,
    pub message_id: Option<String>,
    pub guild_card_anchor_matches: Vec<usize>,
    pub record_candidate_offset: Option<usize>,
    pub record_candidate_size: Option<usize>,
    pub record_candidate_sha256: Option<String>,
    pub record_candidate_nonzero_bytes: Option<usize>,
    pub record_candidate_anchor_matches: Vec<usize>,
}

#[derive(Debug, Serialize)]
pub struct CecSourceReport {
    pub root: PathBuf,
    pub inbox: CecBoxReport,
    pub outbox: CecBoxReport,
    pub messages: Vec<CecMessageReport>,
    pub source_slot: Option<PathBuf>,
}

#[derive(Debug, Serialize)]
pub struct CemuCecReport {
    pub path: PathBuf,
    pub size: usize,
    pub sha256: String,
    pub logical_source_size: Option<u32>,
    pub payload_size: usize,
    pub nonzero_payload_bytes: usize,
    pub first_nonzero_payload_offset: Option<usize>,
    pub record_area_offset: usize,
    pub record_slot_size: usize,
    pub record_slot_count: usize,
    pub expected_layout: bool,
    pub is_empty: bool,
}

#[derive(Debug, Serialize)]
pub struct CecInspectionReport {
    pub source: CecSourceReport,
    pub target: Option<CemuCecReport>,
    pub status: &'static str,
}

#[derive(Debug, Clone)]
pub struct CecRecordSource {
    pub message_file: PathBuf,
    pub record: Vec<u8>,
    pub sha256: String,
}

#[derive(Debug, Clone)]
pub struct CecConversion {
    pub bytes: Vec<u8>,
    pub records: Vec<CecRecordSource>,
    pub slots: Vec<usize>,
    /// Order-independent fingerprint of every valid received source record
    /// observed while planning this conversion, including records already
    /// present in the target cache.
    pub source_record_set_sha256: String,
    pub before_sha256: String,
    pub after_sha256: String,
}

/// Optional CEC hashes captured by the immediately preceding Dry Run.
///
/// The source fingerprint covers the complete deduplicated received-record
/// set. The target fingerprint is the current `cec` bytes, or the stable
/// canonical empty Cemu container when the file does not yet exist.
#[derive(Debug, Clone, Copy, Default)]
pub struct CecInstallExpectations<'a> {
    pub source_record_set_sha256: Option<&'a str>,
    pub target_sha256: Option<&'a str>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CecInstallManifest {
    pub version: u32,
    pub source_dir: PathBuf,
    pub source_record_sha256: Vec<String>,
    pub installed_sha256: String,
    pub previous_sha256: Option<String>,
    pub target: PathBuf,
    pub backup: Option<PathBuf>,
}

#[derive(Debug, Serialize)]
pub struct CecInstallResult {
    pub backup: Option<PathBuf>,
    pub manifest: PathBuf,
    pub installed_sha256: String,
}

/// Actual lock-scoped plan and result for a CEC import.
#[derive(Debug)]
pub struct CecInstalledConversion {
    pub conversion: CecConversion,
    pub install: CecInstallResult,
}

/// Serializes all mutations of one Cemu CEC target directory.
///
/// Both installation and rollback acquire this create-new lock before reading
/// the manifest or target. This prevents a second transaction from treating a
/// partially committed first transaction as its own prior state.
struct CecInstallLock {
    path: PathBuf,
    _file: File,
}

impl CecInstallLock {
    fn acquire(parent: &Path) -> Result<Self, ConversionError> {
        let path = parent.join(".cec.mh3g-install.lock");
        let mut file = match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                return Err(ConversionError::UnsafeInstall(format!(
                    "CEC installation is already locked: {}",
                    path.display()
                )));
            }
            Err(error) => return io_at_path(Err(error), "creating CEC install lock", &path),
        };
        if let Err(error) =
            writeln!(file, "pid={}", std::process::id()).and_then(|_| file.sync_all())
        {
            drop(file);
            let _ = fs::remove_file(&path);
            return io_at_path(Err(error), "writing CEC install lock", &path);
        }
        Ok(Self { path, _file: file })
    }
}

impl Drop for CecInstallLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

#[derive(Debug, Clone, Copy)]
struct ParsedBoxInfo {
    magic: u16,
    box_info_size: u32,
    max_box_size: u32,
    box_size: u32,
    max_message_count: u32,
    declared_message_count: u32,
    max_batch_size: u32,
    max_message_size: u32,
}

fn read_u16(bytes: &[u8], offset: usize) -> Option<u16> {
    bytes
        .get(offset..offset + 2)
        .map(|value| u16::from_le_bytes([value[0], value[1]]))
}

fn read_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    bytes
        .get(offset..offset + 4)
        .map(|value| u32::from_le_bytes([value[0], value[1], value[2], value[3]]))
}

fn read_u32_be(bytes: &[u8], offset: usize) -> Option<u32> {
    bytes
        .get(offset..offset + 4)
        .map(|value| u32::from_be_bytes([value[0], value[1], value[2], value[3]]))
}

fn window_matches(bytes: &[u8], needle: &[u8]) -> Vec<usize> {
    if needle.is_empty() {
        return Vec::new();
    }
    bytes
        .windows(needle.len())
        .enumerate()
        .filter_map(|(offset, window)| (window == needle).then_some(offset))
        .collect()
}

fn parse_box_info(bytes: &[u8]) -> Option<ParsedBoxInfo> {
    (bytes.len() >= BOX_INFO_SIZE).then(|| ParsedBoxInfo {
        magic: read_u16(bytes, 0).unwrap_or_default(),
        box_info_size: read_u32(bytes, 4).unwrap_or_default(),
        max_box_size: read_u32(bytes, 8).unwrap_or_default(),
        box_size: read_u32(bytes, 12).unwrap_or_default(),
        max_message_count: read_u32(bytes, 16).unwrap_or_default(),
        declared_message_count: read_u32(bytes, 20).unwrap_or_default(),
        max_batch_size: read_u32(bytes, 24).unwrap_or_default(),
        max_message_size: read_u32(bytes, 28).unwrap_or_default(),
    })
}

fn box_report(directory: &Path) -> Result<CecBoxReport, ConversionError> {
    let info_path = directory.join("BoxInfo_____");
    let info_bytes = io_at_path(
        fs::read(&info_path),
        "reading CEC mailbox metadata",
        &info_path,
    )?;
    let parsed = parse_box_info(&info_bytes);
    let mut message_files = io_at_path(
        fs::read_dir(directory),
        "listing CEC mailbox directory",
        directory,
    )?
    .filter_map(Result::ok)
    .filter_map(|entry| {
        let file_type = entry.file_type().ok()?;
        let name = entry.file_name().into_string().ok()?;
        (file_type.is_file() && name.starts_with('_')).then_some(name)
    })
    .collect::<Vec<_>>();
    message_files.sort();

    Ok(CecBoxReport {
        path: info_path,
        size: info_bytes.len(),
        magic: parsed.map(|value| format!("0x{:04x}", value.magic)),
        box_info_size: parsed.map(|value| value.box_info_size),
        max_box_size: parsed.map(|value| value.max_box_size),
        box_size: parsed.map(|value| value.box_size),
        max_message_count: parsed.map(|value| value.max_message_count),
        declared_message_count: parsed.map(|value| value.declared_message_count),
        max_batch_size: parsed.map(|value| value.max_batch_size),
        max_message_size: parsed.map(|value| value.max_message_size),
        actual_message_count: message_files.len(),
        message_files,
        header_valid: parsed.is_some_and(|value| value.magic == 0x6262),
    })
}

fn parse_message(
    path: &Path,
    source_slot: Option<&[u8]>,
    box_name: &str,
) -> Result<CecMessageReport, ConversionError> {
    let bytes = io_at_path(fs::read(path), "reading CEC message", path)?;
    let header_valid = bytes.len() >= MESSAGE_HEADER_SIZE && read_u16(&bytes, 0) == Some(0x6060);
    let source_anchor = source_slot
        .and_then(|slot| {
            slot.get(MH3G_GUILD_CARD_OFFSET..MH3G_GUILD_CARD_OFFSET + GUILD_CARD_ANCHOR_SIZE)
        })
        .filter(|anchor| anchor.iter().any(|byte| *byte != 0));
    let guild_card_anchor_matches = source_anchor
        .map(|anchor| window_matches(&bytes, anchor))
        .unwrap_or_default();
    let (
        record_candidate_offset,
        record_candidate_size,
        record_candidate_sha256,
        record_candidate_nonzero_bytes,
        record_candidate_anchor_matches,
    ) = match (read_u32(&bytes, 8), read_u32(&bytes, 12)) {
        (Some(header_size), Some(body_size)) => {
            let start = usize::try_from(header_size)
                .ok()
                .and_then(|value| value.checked_add(CEC_SOURCE_RECORD_PREFIX_SIZE));
            let candidate = start.and_then(|start| {
                let end = start.checked_add(CEMU_RECORD_SLOT_SIZE)?;
                (end <= bytes.len()
                    && usize::try_from(body_size).ok()?
                        == CEC_SOURCE_RECORD_PREFIX_SIZE + CEMU_RECORD_SLOT_SIZE)
                    .then_some((start, &bytes[start..end]))
            });
            match candidate {
                Some((start, record)) => (
                    Some(start),
                    Some(record.len()),
                    Some(sha256_hex(record)),
                    Some(record.iter().filter(|byte| **byte != 0).count()),
                    source_anchor
                        .map(|anchor| window_matches(record, anchor))
                        .unwrap_or_default(),
                ),
                None => (None, None, None, None, Vec::new()),
            }
        }
        _ => (None, None, None, None, Vec::new()),
    };

    Ok(CecMessageReport {
        box_name: box_name.to_owned(),
        file: path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_owned(),
        size: bytes.len(),
        sha256: sha256_hex(&bytes),
        header_valid,
        magic: read_u16(&bytes, 0).map(|value| format!("0x{:04x}", value)),
        message_size: read_u32(&bytes, 4),
        header_size: read_u32(&bytes, 8),
        body_size: read_u32(&bytes, 12),
        title_id: read_u32(&bytes, 16).map(|value| format!("0x{value:08x}")),
        title_id2: read_u32(&bytes, 20).map(|value| format!("0x{value:08x}")),
        batch_id: read_u32(&bytes, 24),
        message_id: bytes.get(32..40).map(hex::encode),
        guild_card_anchor_matches,
        record_candidate_offset,
        record_candidate_size,
        record_candidate_sha256,
        record_candidate_nonzero_bytes,
        record_candidate_anchor_matches,
    })
}

fn source_report(
    root: &Path,
    source_slot: Option<&Path>,
) -> Result<CecSourceReport, ConversionError> {
    if !root.is_dir() {
        return Err(ConversionError::InvalidSave(format!(
            "3DS CEC mailbox is not a directory: {}",
            root.display()
        )));
    }
    let inbox_dir = root.join("InBox___");
    let outbox_dir = root.join("OutBox__");
    for directory in [&inbox_dir, &outbox_dir] {
        if !directory.join("BoxInfo_____").is_file() {
            return Err(ConversionError::InvalidSave(format!(
                "3DS CEC mailbox is missing BoxInfo_____: {}",
                directory.display()
            )));
        }
    }

    let slot_bytes = source_slot
        .map(|path| io_at_path(fs::read(path), "reading CEC source save slot", path))
        .transpose()?;
    let inbox = box_report(&inbox_dir)?;
    let outbox = box_report(&outbox_dir)?;
    let mut messages = Vec::new();
    for (box_name, directory, report) in [
        ("InBox___", &inbox_dir, &inbox),
        ("OutBox__", &outbox_dir, &outbox),
    ] {
        messages.extend(
            report
                .message_files
                .iter()
                .map(|name| parse_message(&directory.join(name), slot_bytes.as_deref(), box_name))
                .collect::<Result<Vec<_>, _>>()?,
        );
    }

    Ok(CecSourceReport {
        root: root.to_path_buf(),
        inbox,
        outbox,
        messages,
        source_slot: source_slot.map(Path::to_path_buf),
    })
}

fn message_paths(directory: &Path) -> Result<Vec<PathBuf>, ConversionError> {
    let mut paths = io_at_path(
        fs::read_dir(directory),
        "listing received CEC messages",
        directory,
    )?
    .filter_map(Result::ok)
    .filter_map(|entry| {
        let file_type = entry.file_type().ok()?;
        let name = entry.file_name().into_string().ok()?;
        (file_type.is_file() && name.starts_with('_')).then_some(entry.path())
    })
    .collect::<Vec<_>>();
    paths.sort();
    Ok(paths)
}

fn read_mh3g_record(path: &Path) -> Result<Option<Vec<u8>>, ConversionError> {
    let bytes = io_at_path(fs::read(path), "reading received CEC message", path)?;
    if bytes.len() < MESSAGE_HEADER_SIZE || read_u16(&bytes, 0) != Some(0x6060) {
        return Ok(None);
    }
    let Some(message_size) = read_u32(&bytes, 4).and_then(|value| usize::try_from(value).ok())
    else {
        return Ok(None);
    };
    let Some(header_size) = read_u32(&bytes, 8).and_then(|value| usize::try_from(value).ok())
    else {
        return Ok(None);
    };
    let Some(body_size) = read_u32(&bytes, 12).and_then(|value| usize::try_from(value).ok()) else {
        return Ok(None);
    };
    if message_size != bytes.len()
        || header_size < MESSAGE_HEADER_SIZE
        || header_size
            .checked_add(body_size)
            .is_none_or(|end| end > bytes.len())
        || read_u32(&bytes, 20) != Some(0)
    {
        return Ok(None);
    }
    if read_u32(&bytes, 16) != Some(MH3G_TITLE_ID) || body_size != MH3G_BODY_SIZE {
        return Ok(None);
    }
    let Some(start) = header_size.checked_add(CEC_SOURCE_RECORD_PREFIX_SIZE) else {
        return Ok(None);
    };
    let Some(end) = start.checked_add(CEMU_RECORD_SLOT_SIZE) else {
        return Ok(None);
    };
    if end > bytes.len() {
        return Ok(None);
    }
    let record = &bytes[start..end];
    if record.iter().all(|byte| *byte == 0) {
        return Ok(None);
    }
    Ok(Some(record.to_vec()))
}

/// Collect received MH3G StreetPass records from the 3DS CEC inbox.
///
/// The 3DS message wrapper is not copied into Cemu. The observed MH3G message
/// body has an 8-byte prefix followed by one fixed 0x2A00-byte record, which is
/// the same slot size used by Cemu's 50-record cache. Outgoing messages are
/// deliberately not candidates: an `OutBox__` record describes the source
/// hunter's own transmission, not a card received from another hunter.
pub fn collect_received_mh3g_records(root: &Path) -> Result<Vec<CecRecordSource>, ConversionError> {
    if !root.is_dir() {
        return Err(ConversionError::InvalidSave(format!(
            "3DS CEC mailbox is not a directory: {}",
            root.display()
        )));
    }
    let mut records = Vec::new();
    let mut seen = BTreeSet::new();
    let directory = root.join("InBox___");
    if !directory.join("BoxInfo_____").is_file() {
        return Err(ConversionError::InvalidSave(format!(
            "3DS CEC mailbox is missing InBox___/BoxInfo_____: {}",
            directory.display()
        )));
    }
    for path in message_paths(&directory)? {
        let Some(record) = read_mh3g_record(&path)? else {
            continue;
        };
        let hash = sha256_hex(&record);
        if seen.insert(hash.clone()) {
            records.push(CecRecordSource {
                message_file: path,
                record,
                sha256: hash,
            });
        }
    }
    Ok(records)
}

/// Return an order-independent fingerprint for the received source records.
///
/// The input has normally already been deduplicated by
/// `collect_received_mh3g_records`, but this function applies set semantics
/// itself so callers cannot accidentally turn mailbox ordering into a write
/// precondition. The domain separator makes the digest distinct from a raw
/// record or Cemu container digest.
pub fn source_record_set_sha256(records: &[CecRecordSource]) -> String {
    let hashes = records
        .iter()
        .map(|record| record.sha256.as_str())
        .collect::<BTreeSet<_>>();
    let mut canonical = b"mh3g-cec-source-record-set-v1\0".to_vec();
    for hash in hashes {
        canonical.extend_from_slice(hash.as_bytes());
        canonical.push(0);
    }
    sha256_hex(&canonical)
}

fn validate_cemu_cec_bytes(bytes: &[u8]) -> Result<(), ConversionError> {
    let expected_header = build_jp_cemu_header("cec", CEMU_CEC_PAYLOAD_SIZE)?;
    if bytes.len() != CEMU_HEADER_SIZE + CEMU_CEC_PAYLOAD_SIZE
        || bytes.get(..CEMU_HEADER_SIZE) != Some(expected_header.as_slice())
    {
        return Err(ConversionError::InvalidSave(format!(
            "unrecognized Cemu MH3G cec container (expected {} bytes with 0x2B profile)",
            CEMU_HEADER_SIZE + CEMU_CEC_PAYLOAD_SIZE
        )));
    }
    Ok(())
}

/// Validate an in-memory Japanese MH3G Cemu `cec` container without reading
/// or mutating emulator state.
pub fn validate_cemu_cec(bytes: &[u8]) -> Result<(), ConversionError> {
    validate_cemu_cec_bytes(bytes)
}

pub fn empty_cemu_cec() -> Result<Vec<u8>, ConversionError> {
    let header = build_jp_cemu_header("cec", CEMU_CEC_PAYLOAD_SIZE)?;
    let mut bytes = Vec::with_capacity(CEMU_HEADER_SIZE + CEMU_CEC_PAYLOAD_SIZE);
    bytes.extend_from_slice(&header);
    bytes.resize(CEMU_HEADER_SIZE + CEMU_CEC_PAYLOAD_SIZE, 0);
    Ok(bytes)
}

fn cemu_record_range(slot: usize) -> Result<std::ops::Range<usize>, ConversionError> {
    if slot >= CEMU_RECORD_SLOT_COUNT {
        return Err(ConversionError::InvalidSave(format!(
            "Cemu cec slot is out of range: {slot} (max {})",
            CEMU_RECORD_SLOT_COUNT - 1
        )));
    }
    let start = CEMU_HEADER_SIZE
        .checked_add(CEMU_RECORD_AREA_OFFSET)
        .and_then(|value| value.checked_add(slot * CEMU_RECORD_SLOT_SIZE))
        .ok_or_else(|| ConversionError::InvalidSave("Cemu cec slot offset overflow".to_owned()))?;
    Ok(start..start + CEMU_RECORD_SLOT_SIZE)
}

/// Insert all source records into empty fixed Cemu slots without changing the
/// outer container or any existing non-empty slot.
pub fn convert_cec_records(
    source_dir: &Path,
    target_bytes: &[u8],
    requested_slot: Option<usize>,
) -> Result<CecConversion, ConversionError> {
    validate_cemu_cec_bytes(target_bytes)?;
    let records = collect_received_mh3g_records(source_dir)?;
    convert_collected_cec_records(records, target_bytes, requested_slot)
}

/// Repair a Cemu CEC cache without treating the independently selected output
/// as the read authority.
///
/// Received records produced by one of the supported historical converter
/// revisions are replaced in their existing slots with the current mapping.
/// Already-current records and records unrelated to the supplied 3DS mailbox
/// are preserved byte-for-byte. Missing source records are inserted into empty
/// slots using the same deterministic assignment as a fresh conversion.
pub fn repair_cec_records(
    source_dir: &Path,
    current_bytes: &[u8],
    requested_slot: Option<usize>,
) -> Result<CecConversion, ConversionError> {
    validate_cemu_cec_bytes(current_bytes)?;
    let mut records = collect_received_mh3g_records(source_dir)?;
    if records.is_empty() {
        return Err(ConversionError::InvalidSave(
            "3DS CEC InBox___ contains no non-empty received MH3G records".to_owned(),
        ));
    }
    records.sort_by(|left, right| left.sha256.cmp(&right.sha256));
    let source_record_set_sha256 = source_record_set_sha256(&records);
    let before_sha256 = sha256_hex(current_bytes);
    let mut output = current_bytes.to_vec();
    let mut changed_records = Vec::new();
    let mut changed_slots = Vec::new();
    let mut missing_records = Vec::new();

    for mut record in records {
        let source = record.record.clone();
        let current = convert_cec_record(&source)?;
        let already_current = (0..CEMU_RECORD_SLOT_COUNT).any(|slot| {
            cemu_record_range(slot)
                .ok()
                .is_some_and(|range| output[range] == current)
        });
        if already_current {
            continue;
        }

        let historical = ConverterRevision::ALL
            .into_iter()
            .map(|revision| convert_cec_record_for_revision(&source, revision))
            .collect::<Result<Vec<_>, ConversionError>>()?;
        let historical_slot = (0..CEMU_RECORD_SLOT_COUNT).find(|slot| {
            cemu_record_range(*slot).ok().is_some_and(|range| {
                historical
                    .iter()
                    .any(|candidate| candidate != &current && output[range.clone()] == *candidate)
            })
        });
        record.record = current;
        if let Some(slot) = historical_slot {
            let range = cemu_record_range(slot)?;
            output[range].copy_from_slice(&record.record);
            changed_slots.push(slot);
            changed_records.push(record);
        } else {
            missing_records.push(record);
        }
    }

    let mut empty_slots = (0..CEMU_RECORD_SLOT_COUNT)
        .filter(|slot| {
            cemu_record_range(*slot)
                .ok()
                .is_some_and(|range| output[range].iter().all(|byte| *byte == 0))
        })
        .collect::<Vec<_>>();
    if let Some(start) = requested_slot {
        if start >= CEMU_RECORD_SLOT_COUNT {
            return Err(ConversionError::InvalidSave(format!(
                "Cemu cec slot is out of range: {start} (max {})",
                CEMU_RECORD_SLOT_COUNT - 1
            )));
        }
        empty_slots.retain(|slot| *slot >= start);
    }
    if missing_records.len() > empty_slots.len() {
        return Err(ConversionError::UnsafeInstall(format!(
            "Cemu cec has {} empty slots but {} MH3G records need import",
            empty_slots.len(),
            missing_records.len()
        )));
    }
    for (record, slot) in missing_records.into_iter().zip(empty_slots) {
        let range = cemu_record_range(slot)?;
        output[range].copy_from_slice(&record.record);
        changed_slots.push(slot);
        changed_records.push(record);
    }

    Ok(CecConversion {
        after_sha256: sha256_hex(&output),
        bytes: output,
        records: changed_records,
        slots: changed_slots,
        source_record_set_sha256,
        before_sha256,
    })
}

fn convert_cec_record(source: &[u8]) -> Result<Vec<u8>, ConversionError> {
    let mut converted = source.to_vec();
    for slot in 0..CEC_GUILD_CARD_SLOT_COUNT {
        let start = slot * GUILD_CARD_SLOT_SIZE;
        let end = start + GUILD_CARD_SLOT_SIZE;
        apply_japanese_wiiu_guild_card_slot_corrections(
            &source[start..end],
            &mut converted[start..end],
        )?;
    }
    Ok(converted)
}

fn convert_cec_record_for_revision(
    source: &[u8],
    revision: ConverterRevision,
) -> Result<Vec<u8>, ConversionError> {
    let mut converted = source.to_vec();
    for slot in 0..CEC_GUILD_CARD_SLOT_COUNT {
        let start = slot * GUILD_CARD_SLOT_SIZE;
        let end = start + GUILD_CARD_SLOT_SIZE;
        apply_japanese_wiiu_guild_card_slot_corrections_for_revision(
            &source[start..end],
            &mut converted[start..end],
            revision,
        )?;
    }
    Ok(converted)
}

fn convert_collected_cec_records(
    mut records: Vec<CecRecordSource>,
    target_bytes: &[u8],
    requested_slot: Option<usize>,
) -> Result<CecConversion, ConversionError> {
    validate_cemu_cec_bytes(target_bytes)?;
    if records.is_empty() {
        return Err(ConversionError::InvalidSave(
            "3DS CEC InBox___ contains no non-empty received MH3G records".to_owned(),
        ));
    }

    // A CEC Dry Run authorizes the set of raw received records, not their
    // mailbox filenames. Canonicalize before fixed Cemu slot assignment so
    // swapping two mailbox files cannot change the approved output while
    // leaving its record-set fingerprint unchanged.
    records.sort_by(|left, right| left.sha256.cmp(&right.sha256));
    let source_record_set_sha256 = source_record_set_sha256(&records);
    let mut output = target_bytes.to_vec();
    let before_sha256 = sha256_hex(target_bytes);
    let records = records
        .into_iter()
        .map(|mut record| {
            let source = record.record.clone();
            record.record = convert_cec_record(&source)?;
            Ok(record)
        })
        .collect::<Result<Vec<_>, ConversionError>>()?;
    let mut pending = Vec::new();
    for record in records {
        let already_present = (0..CEMU_RECORD_SLOT_COUNT).any(|slot| {
            cemu_record_range(slot)
                .ok()
                .is_some_and(|range| output[range] == record.record)
        });
        if !already_present {
            pending.push(record);
        }
    }

    let mut empty_slots = (0..CEMU_RECORD_SLOT_COUNT)
        .filter(|slot| {
            cemu_record_range(*slot)
                .ok()
                .is_some_and(|range| output[range].iter().all(|byte| *byte == 0))
        })
        .collect::<Vec<_>>();
    if let Some(start) = requested_slot {
        if start >= CEMU_RECORD_SLOT_COUNT {
            return Err(ConversionError::InvalidSave(format!(
                "Cemu cec slot is out of range: {start} (max {})",
                CEMU_RECORD_SLOT_COUNT - 1
            )));
        }
        empty_slots.retain(|slot| *slot >= start);
    }
    if pending.len() > empty_slots.len() {
        return Err(ConversionError::UnsafeInstall(format!(
            "Cemu cec has {} empty slots but {} MH3G records need import",
            empty_slots.len(),
            pending.len()
        )));
    }

    let slots = empty_slots
        .into_iter()
        .take(pending.len())
        .collect::<Vec<_>>();
    for (record, slot) in pending.iter().zip(slots.iter()) {
        let range = cemu_record_range(*slot)?;
        output[range].copy_from_slice(&record.record);
    }

    Ok(CecConversion {
        after_sha256: sha256_hex(&output),
        bytes: output,
        records: pending,
        slots,
        source_record_set_sha256,
        before_sha256,
    })
}

/// Atomically install a converted Cemu cec cache, retaining a hash-addressed
/// backup and a manifest next to the target.
pub fn install_cec(
    source_dir: &Path,
    target: &Path,
    conversion: &CecConversion,
) -> Result<CecInstallResult, ConversionError> {
    install_cec_with(
        source_dir,
        target,
        conversion,
        &PlatformProcessProbe::default(),
    )
}

struct CecInstallContext {
    parent: PathBuf,
    _install_lock: CecInstallLock,
    previous: Option<Vec<u8>>,
    previous_sha256: Option<String>,
}

fn validate_cec_install_expectations(
    expectations: CecInstallExpectations<'_>,
) -> Result<(), ConversionError> {
    for (label, value) in [
        ("source record set", expectations.source_record_set_sha256),
        ("target", expectations.target_sha256),
    ] {
        if let Some(value) = value
            && !(value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()))
        {
            return Err(ConversionError::InvalidSave(format!(
                "expected {label} SHA-256 is not a SHA-256 hex digest"
            )));
        }
    }
    Ok(())
}

fn ensure_expected_cec_hash(
    expected: Option<&str>,
    observed: &str,
    label: &str,
) -> Result<(), ConversionError> {
    if let Some(expected) = expected
        && !expected.eq_ignore_ascii_case(observed)
    {
        return Err(ConversionError::UnsafeInstall(format!(
            "{label} SHA-256 does not match the expected dry-run value"
        )));
    }
    Ok(())
}

fn prepare_cec_install(
    target: &Path,
    probe: &dyn ProcessProbe,
) -> Result<CecInstallContext, ConversionError> {
    if target.file_name().and_then(|name| name.to_str()) != Some("cec") {
        return Err(ConversionError::InvalidSave(format!(
            "Cemu CEC target must be named cec: {}",
            target.display()
        )));
    }
    let parent = target.parent().ok_or_else(|| {
        ConversionError::InvalidSave(format!("CEC target has no parent: {}", target.display()))
    })?;
    if !parent.is_dir() {
        return Err(ConversionError::InvalidSave(format!(
            "CEC target parent is not a directory: {}",
            parent.display()
        )));
    }
    let install_lock = CecInstallLock::acquire(parent)?;
    let target_is_symlink = match fs::symlink_metadata(target) {
        Ok(metadata) => metadata.file_type().is_symlink(),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
        Err(error) => {
            return io_at_path(Err(error), "reading Cemu CEC target metadata", target);
        }
    };
    if target_is_symlink {
        return Err(ConversionError::InvalidSave(
            "CEC target cannot be a symlink".to_owned(),
        ));
    }
    if let Some(name) = probe.matching_process()? {
        return Err(ConversionError::UnsafeInstall(format!(
            "emulator process is running: {name}"
        )));
    }
    let manifest_path = parent.join(".cec.mh3g-install.json");
    match fs::symlink_metadata(&manifest_path) {
        Ok(_) => {
            return Err(ConversionError::UnsafeInstall(format!(
                "CEC install manifest already exists: {}",
                manifest_path.display()
            )));
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return io_at_path(
                Err(error),
                "reading CEC install manifest metadata",
                &manifest_path,
            );
        }
    }

    let previous = match fs::read(target) {
        Ok(bytes) => {
            validate_cemu_cec_bytes(&bytes)?;
            Some(bytes)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return io_at_path(Err(error), "reading Cemu CEC target", target),
    };
    let previous_sha256 = previous.as_deref().map(sha256_hex);
    Ok(CecInstallContext {
        parent: parent.to_path_buf(),
        _install_lock: install_lock,
        previous,
        previous_sha256,
    })
}

fn observed_cec_target_sha256(context: &CecInstallContext) -> Result<String, ConversionError> {
    match context.previous_sha256.as_deref() {
        Some(hash) => Ok(hash.to_owned()),
        None => Ok(sha256_hex(&empty_cemu_cec()?)),
    }
}

pub fn install_cec_with(
    source_dir: &Path,
    target: &Path,
    conversion: &CecConversion,
    probe: &dyn ProcessProbe,
) -> Result<CecInstallResult, ConversionError> {
    let context = prepare_cec_install(target, probe)?;
    let observed_sha256 = observed_cec_target_sha256(&context)?;
    if observed_sha256 != conversion.before_sha256 {
        return Err(ConversionError::UnsafeInstall(
            "CEC target changed after the conversion plan was created".to_owned(),
        ));
    }
    install_cec_transaction(source_dir, target, conversion, &context)
}

/// Install a conversion derived from an independent read-only Cemu baseline.
///
/// Unlike [`install_cec_with`], `conversion.before_sha256` identifies the
/// separately selected current/reference cache rather than the output path.
/// The output is therefore guarded by its own expected hash before the
/// precomputed bytes are installed. A missing output uses the same canonical
/// empty-container fingerprint as the existing CEC export flow.
pub fn install_precomputed_cec_with_target_expectation(
    source_dir: &Path,
    target: &Path,
    conversion: &CecConversion,
    expected_target_sha256: &str,
) -> Result<CecInstallResult, ConversionError> {
    validate_cec_install_expectations(CecInstallExpectations {
        source_record_set_sha256: None,
        target_sha256: Some(expected_target_sha256),
    })?;
    validate_cemu_cec_bytes(&conversion.bytes)?;
    if sha256_hex(&conversion.bytes) != conversion.after_sha256 {
        return Err(ConversionError::UnsafeInstall(
            "precomputed CEC conversion bytes do not match their planned hash".to_owned(),
        ));
    }
    let context = prepare_cec_install(target, &PlatformProcessProbe::default())?;
    let observed_target_sha256 = observed_cec_target_sha256(&context)?;
    ensure_expected_cec_hash(
        Some(expected_target_sha256),
        &observed_target_sha256,
        "output target",
    )?;
    install_cec_transaction(source_dir, target, conversion, &context)
}

/// Rebuild and install a CEC conversion while holding the per-target lock.
///
/// This is the write counterpart to a CEC Dry Run: it re-reads both the Cemu
/// target and the complete 3DS received-record set after obtaining the lock,
/// verifies the caller's fingerprints, and only then derives the bytes to
/// install from that in-memory snapshot.
pub fn install_cec_from_source_with_expectations(
    source_dir: &Path,
    target: &Path,
    requested_slot: Option<usize>,
    expectations: CecInstallExpectations<'_>,
) -> Result<CecInstalledConversion, ConversionError> {
    install_cec_from_source_with_expectations_and_probe(
        source_dir,
        target,
        requested_slot,
        expectations,
        &PlatformProcessProbe::default(),
    )
}

fn install_cec_from_source_with_expectations_and_probe(
    source_dir: &Path,
    target: &Path,
    requested_slot: Option<usize>,
    expectations: CecInstallExpectations<'_>,
    probe: &dyn ProcessProbe,
) -> Result<CecInstalledConversion, ConversionError> {
    validate_cec_install_expectations(expectations)?;
    let context = prepare_cec_install(target, probe)?;
    let observed_target_sha256 = observed_cec_target_sha256(&context)?;
    ensure_expected_cec_hash(
        expectations.target_sha256,
        &observed_target_sha256,
        "target",
    )?;

    let source_records = collect_received_mh3g_records(source_dir)?;
    let observed_source_record_set_sha256 = source_record_set_sha256(&source_records);
    ensure_expected_cec_hash(
        expectations.source_record_set_sha256,
        &observed_source_record_set_sha256,
        "source record set",
    )?;

    let empty_target: Vec<u8>;
    let target_bytes = match context.previous.as_deref() {
        Some(bytes) => bytes,
        None => {
            empty_target = empty_cemu_cec()?;
            &empty_target
        }
    };
    let conversion = convert_collected_cec_records(source_records, target_bytes, requested_slot)?;
    if conversion.source_record_set_sha256 != observed_source_record_set_sha256 {
        return Err(ConversionError::UnsafeInstall(
            "CEC source record set changed while the write plan was created".to_owned(),
        ));
    }
    if conversion.before_sha256 != observed_target_sha256 {
        return Err(ConversionError::UnsafeInstall(
            "CEC target changed while the write plan was created".to_owned(),
        ));
    }
    let install = install_cec_transaction(source_dir, target, &conversion, &context)?;
    Ok(CecInstalledConversion {
        conversion,
        install,
    })
}

fn install_cec_transaction(
    source_dir: &Path,
    target: &Path,
    conversion: &CecConversion,
    context: &CecInstallContext,
) -> Result<CecInstallResult, ConversionError> {
    let parent = context.parent.as_path();
    let manifest_path = parent.join(".cec.mh3g-install.json");
    let previous = context.previous.as_deref();
    let previous_sha256 = context.previous_sha256.clone();
    let backup = previous_sha256
        .as_deref()
        .map(|hash| parent.join(format!(".cec.mh3g-backup-{hash}")));
    let manifest = CecInstallManifest {
        version: 1,
        source_dir: source_dir.to_path_buf(),
        source_record_sha256: conversion
            .records
            .iter()
            .map(|record| record.sha256.clone())
            .collect(),
        installed_sha256: conversion.after_sha256.clone(),
        previous_sha256,
        target: target.to_path_buf(),
        backup: backup.clone(),
    };
    let manifest_bytes = serde_json::to_vec_pretty(&manifest)?;
    let temporary = unique_path(target, "cec-tmp");
    let mut backup_created = false;
    let mut temporary_created = false;
    let mut target_installed = false;
    let mut manifest_created = false;
    let result = (|| {
        if let (Some(path), Some(bytes)) = (&backup, previous) {
            match fs::symlink_metadata(path) {
                Ok(metadata) if metadata.file_type().is_file() => {
                    let existing = io_at_path(fs::read(path), "reading existing CEC backup", path)?;
                    if existing != bytes {
                        return Err(ConversionError::UnsafeInstall(format!(
                            "CEC backup path already contains different bytes: {}",
                            path.display()
                        )));
                    }
                }
                Ok(_) => {
                    return Err(ConversionError::UnsafeInstall(format!(
                        "CEC backup path must be a regular non-symlink file: {}",
                        path.display()
                    )));
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    write_new_file(path, bytes)?;
                    backup_created = true;
                }
                Err(error) => {
                    return io_at_path(Err(error), "reading existing CEC backup metadata", path);
                }
            }
        }

        write_new_file(&temporary, &conversion.bytes)?;
        temporary_created = true;
        io_at_path(
            fs::rename(&temporary, target),
            "installing staged CEC cache",
            target,
        )?;
        temporary_created = false;
        target_installed = true;
        sync_directory(parent)?;
        write_new_file(&manifest_path, &manifest_bytes)?;
        manifest_created = true;
        sync_directory(parent)?;
        Ok(CecInstallResult {
            backup: backup.clone(),
            manifest: manifest_path.clone(),
            installed_sha256: conversion.after_sha256.clone(),
        })
    })();

    match result {
        Ok(installed) => Ok(installed),
        Err(install_error) => {
            let mut cleanup_errors = Vec::new();
            if temporary_created && let Err(error) = remove_if_regular_file(&temporary) {
                cleanup_errors.push(format!("remove staged CEC cache: {error}"));
            }

            let restore_result = if target_installed {
                if let Some(previous) = previous {
                    atomic_replace(target, previous)
                } else {
                    remove_if_regular_file(target)
                }
            } else {
                Ok(())
            };
            let target_restored = restore_result.is_ok();
            if let Err(error) = restore_result {
                cleanup_errors.push(format!("restore prior CEC cache: {error}"));
            }

            if target_restored {
                if manifest_created && let Err(error) = remove_if_regular_file(&manifest_path) {
                    cleanup_errors.push(format!("remove new CEC manifest: {error}"));
                }
                if backup_created
                    && let Some(path) = backup.as_deref()
                    && let Err(error) = remove_if_regular_file(path)
                {
                    cleanup_errors.push(format!("remove new CEC backup: {error}"));
                }
            } else if !manifest_created {
                match write_new_file(&manifest_path, &manifest_bytes) {
                    Ok(()) => manifest_created = true,
                    Err(error) => {
                        cleanup_errors.push(format!("publish CEC recovery manifest: {error}"));
                    }
                }
            }

            if let Err(error) = sync_directory(parent) {
                cleanup_errors.push(format!("sync CEC transaction directory: {error}"));
            }

            if cleanup_errors.is_empty() {
                Err(install_error)
            } else {
                let recovery = if manifest_created {
                    format!("; recovery manifest: {}", manifest_path.display())
                } else if let Some(path) = backup.as_deref() {
                    format!("; retained backup: {}", path.display())
                } else {
                    String::new()
                };
                Err(ConversionError::UnsafeInstall(format!(
                    "CEC installation failed: {install_error}; cleanup also failed: {}{recovery}",
                    cleanup_errors.join("; ")
                )))
            }
        }
    }
}

fn validate_cec_manifest_hash(value: &str, label: &str) -> Result<(), ConversionError> {
    if value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err(ConversionError::InvalidSave(format!(
            "CEC manifest {label} hash is not a SHA-256 hex digest"
        )))
    }
}

/// Roll back a prior `install_cec` transaction after verifying the installed
/// hash and the hash-addressed backup.
pub fn rollback_cec(manifest_path: &Path) -> Result<(), ConversionError> {
    rollback_cec_with(manifest_path, &PlatformProcessProbe::default())
}

pub fn rollback_cec_with(
    manifest_path: &Path,
    probe: &dyn ProcessProbe,
) -> Result<(), ConversionError> {
    if manifest_path.file_name().and_then(|name| name.to_str()) != Some(".cec.mh3g-install.json") {
        return Err(ConversionError::InvalidSave(format!(
            "CEC rollback manifest has an unexpected name: {}",
            manifest_path.display()
        )));
    }
    let parent = manifest_path.parent().ok_or_else(|| {
        ConversionError::InvalidSave(format!(
            "CEC rollback manifest has no parent: {}",
            manifest_path.display()
        ))
    })?;
    if !parent.is_dir() {
        return Err(ConversionError::InvalidSave(format!(
            "CEC rollback manifest parent is not a directory: {}",
            parent.display()
        )));
    }
    let _install_lock = CecInstallLock::acquire(parent)?;
    let manifest_metadata = io_at_path(
        fs::symlink_metadata(manifest_path),
        "reading CEC rollback manifest metadata",
        manifest_path,
    )?;
    if !manifest_metadata.file_type().is_file() {
        return Err(ConversionError::InvalidSave(
            "CEC rollback manifest must be a regular non-symlink file".to_owned(),
        ));
    }
    let manifest_bytes = io_at_path(
        fs::read(manifest_path),
        "reading CEC rollback manifest",
        manifest_path,
    )?;
    let manifest: CecInstallManifest = serde_json::from_slice(&manifest_bytes)?;
    if manifest.version != 1 {
        return Err(ConversionError::InvalidSave(format!(
            "unsupported CEC install manifest version: {}",
            manifest.version
        )));
    }
    validate_cec_manifest_hash(&manifest.installed_sha256, "installed")?;
    if manifest.target.file_name().and_then(|name| name.to_str()) != Some("cec")
        || manifest.target.parent() != Some(parent)
    {
        return Err(ConversionError::InvalidSave(
            "CEC rollback target is not bound to the manifest directory".to_owned(),
        ));
    }
    let target_is_symlink = match fs::symlink_metadata(&manifest.target) {
        Ok(metadata) => metadata.file_type().is_symlink(),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
        Err(error) => {
            return io_at_path(
                Err(error),
                "reading CEC rollback target metadata",
                &manifest.target,
            );
        }
    };
    if target_is_symlink {
        return Err(ConversionError::InvalidSave(
            "CEC rollback target cannot be a symlink".to_owned(),
        ));
    }
    if let Some(name) = probe.matching_process()? {
        return Err(ConversionError::UnsafeInstall(format!(
            "emulator process is running: {name}"
        )));
    }

    let current = match fs::read(&manifest.target) {
        Ok(bytes) => Some(bytes),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => {
            return io_at_path(Err(error), "reading CEC rollback target", &manifest.target);
        }
    };
    let current_sha256 = current.as_deref().map(sha256_hex);

    match (&manifest.backup, &manifest.previous_sha256) {
        (Some(backup), Some(previous_sha256)) => {
            validate_cec_manifest_hash(previous_sha256, "previous")?;
            let expected = parent.join(format!(".cec.mh3g-backup-{previous_sha256}"));
            if backup != &expected {
                return Err(ConversionError::InvalidSave(
                    "CEC rollback backup path is not hash-bound".to_owned(),
                ));
            }
            match current_sha256.as_deref() {
                Some(hash) if hash == manifest.installed_sha256 => {
                    let backup_metadata = io_at_path(
                        fs::symlink_metadata(backup),
                        "reading CEC rollback backup metadata",
                        backup,
                    )?;
                    if !backup_metadata.file_type().is_file() {
                        return Err(ConversionError::InvalidSave(
                            "CEC rollback backup must be a regular non-symlink file".to_owned(),
                        ));
                    }
                    let previous =
                        io_at_path(fs::read(backup), "reading CEC rollback backup", backup)?;
                    if sha256_hex(&previous) != *previous_sha256 {
                        return Err(ConversionError::InvalidSave(
                            "CEC rollback backup hash does not match the manifest".to_owned(),
                        ));
                    }
                    atomic_replace(&manifest.target, &previous)?;
                    remove_if_regular_file(backup)?;
                }
                Some(hash) if hash == previous_sha256 => match fs::symlink_metadata(backup) {
                    Ok(metadata) if metadata.file_type().is_file() => {
                        let previous =
                            io_at_path(fs::read(backup), "reading CEC rollback backup", backup)?;
                        if sha256_hex(&previous) != *previous_sha256 {
                            return Err(ConversionError::InvalidSave(
                                "CEC rollback backup hash does not match the manifest".to_owned(),
                            ));
                        }
                        remove_if_regular_file(backup)?;
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Ok(_) => {
                        return Err(ConversionError::InvalidSave(
                            "CEC rollback backup must be a regular non-symlink file".to_owned(),
                        ));
                    }
                    Err(error) => {
                        return io_at_path(
                            Err(error),
                            "reading CEC rollback backup metadata",
                            backup,
                        );
                    }
                },
                _ => {
                    return Err(ConversionError::InvalidSave(
                        "CEC rollback target hash does not match the install manifest".to_owned(),
                    ));
                }
            }
        }
        (None, None) => match current_sha256.as_deref() {
            Some(hash) if hash == manifest.installed_sha256 => {
                remove_if_regular_file(&manifest.target)?;
            }
            None => {}
            _ => {
                return Err(ConversionError::InvalidSave(
                    "CEC rollback target hash does not match the install manifest".to_owned(),
                ));
            }
        },
        _ => {
            return Err(ConversionError::InvalidSave(
                "CEC rollback manifest backup fields are inconsistent".to_owned(),
            ));
        }
    }
    io_at_path(
        fs::remove_file(manifest_path),
        "removing consumed CEC rollback manifest",
        manifest_path,
    )?;
    sync_directory(parent)?;
    Ok(())
}

fn target_report(path: &Path) -> Result<CemuCecReport, ConversionError> {
    let bytes = io_at_path(fs::read(path), "reading Cemu CEC target", path)?;
    validate_cemu_cec_bytes(&bytes).map_err(|_| {
        ConversionError::InvalidSave(format!(
            "unrecognized Cemu MH3G cec container: {}",
            path.display()
        ))
    })?;
    let payload = &bytes[CEMU_HEADER_SIZE..];
    let nonzero_payload_bytes = payload.iter().filter(|byte| **byte != 0).count();
    let first_nonzero_payload_offset = payload.iter().position(|byte| *byte != 0);
    let record_slot_count = payload
        .len()
        .checked_sub(CEMU_RECORD_AREA_OFFSET)
        .filter(|size| size % CEMU_RECORD_SLOT_SIZE == 0)
        .map(|size| size / CEMU_RECORD_SLOT_SIZE)
        .unwrap_or(0);

    Ok(CemuCecReport {
        path: path.to_path_buf(),
        size: bytes.len(),
        sha256: sha256_hex(&bytes),
        logical_source_size: read_u32_be(&bytes, 28),
        payload_size: payload.len(),
        nonzero_payload_bytes,
        first_nonzero_payload_offset,
        record_area_offset: CEMU_RECORD_AREA_OFFSET,
        record_slot_size: CEMU_RECORD_SLOT_SIZE,
        record_slot_count,
        expected_layout: payload.len() == CEMU_CEC_PAYLOAD_SIZE,
        is_empty: nonzero_payload_bytes == 0,
    })
}

pub fn inspect_cec(
    source_dir: PathBuf,
    target: Option<PathBuf>,
    source_slot: Option<PathBuf>,
) -> Result<CecInspectionReport, ConversionError> {
    let source = source_report(&source_dir, source_slot.as_deref())?;
    let target = target.as_deref().map(target_report).transpose()?;
    Ok(CecInspectionReport {
        source,
        target,
        status: "inspected-cec",
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    struct Running;

    impl ProcessProbe for Running {
        fn matching_process(&self) -> Result<Option<String>, ConversionError> {
            Ok(Some("Cemu.exe".to_owned()))
        }
    }

    struct Stopped;

    impl ProcessProbe for Stopped {
        fn matching_process(&self) -> Result<Option<String>, ConversionError> {
            Ok(None)
        }
    }

    fn received_message_with_record(record: &[u8]) -> Vec<u8> {
        assert_eq!(record.len(), CEMU_RECORD_SLOT_SIZE);
        let body_size = CEC_SOURCE_RECORD_PREFIX_SIZE + CEMU_RECORD_SLOT_SIZE;
        let header_size = MESSAGE_HEADER_SIZE;
        let mut message = vec![0_u8; header_size + body_size];
        let message_size = message.len() as u32;
        message[0..2].copy_from_slice(&0x6060_u16.to_le_bytes());
        message[4..8].copy_from_slice(&message_size.to_le_bytes());
        message[8..12].copy_from_slice(&(header_size as u32).to_le_bytes());
        message[12..16].copy_from_slice(&(body_size as u32).to_le_bytes());
        message[16..20].copy_from_slice(&MH3G_TITLE_ID.to_le_bytes());
        message[header_size + CEC_SOURCE_RECORD_PREFIX_SIZE..].copy_from_slice(record);
        message
    }

    #[cfg(windows)]
    #[test]
    fn directory_sync_does_not_open_windows_directories_as_files() {
        let temp = tempdir().unwrap();

        sync_directory(temp.path()).unwrap();
    }

    #[test]
    fn cemu_record_geometry_matches_native_japanese_container() {
        assert_eq!(
            CEMU_CEC_PAYLOAD_SIZE,
            CEMU_RECORD_AREA_OFFSET + CEMU_RECORD_SLOT_COUNT * CEMU_RECORD_SLOT_SIZE
        );
        assert_eq!(CEC_GUILD_CARD_SLOT_COUNT, 3);
        assert_eq!(
            CEMU_RECORD_SLOT_SIZE,
            CEC_GUILD_CARD_SLOT_COUNT * GUILD_CARD_SLOT_SIZE
        );
    }

    #[test]
    fn cec_slot_assignment_is_stable_when_mailbox_filenames_are_swapped() {
        let temp = tempdir().unwrap();
        let inbox = temp.path().join("InBox___");
        fs::create_dir_all(&inbox).unwrap();
        fs::write(inbox.join("BoxInfo_____"), [0_u8; BOX_INFO_SIZE]).unwrap();

        let mut first_record = vec![0_u8; CEMU_RECORD_SLOT_SIZE];
        first_record[0] = 0xA1;
        let mut second_record = vec![0_u8; CEMU_RECORD_SLOT_SIZE];
        second_record[0] = 0xB2;
        let first_message = received_message_with_record(&first_record);
        let second_message = received_message_with_record(&second_record);
        fs::write(inbox.join("_A"), &first_message).unwrap();
        fs::write(inbox.join("_B"), &second_message).unwrap();

        let target = empty_cemu_cec().unwrap();
        let before = convert_cec_records(temp.path(), &target, None).unwrap();

        fs::write(inbox.join("_A"), second_message).unwrap();
        fs::write(inbox.join("_B"), first_message).unwrap();
        let after = convert_cec_records(temp.path(), &target, None).unwrap();

        assert_eq!(
            before.source_record_set_sha256,
            after.source_record_set_sha256
        );
        assert_eq!(before.slots, after.slots);
        assert_eq!(
            before
                .records
                .iter()
                .map(|record| &record.sha256)
                .collect::<Vec<_>>(),
            after
                .records
                .iter()
                .map(|record| &record.sha256)
                .collect::<Vec<_>>()
        );
        assert_eq!(before.bytes, after.bytes);
    }

    #[test]
    fn repair_replaces_a_historical_cec_record_in_place() {
        let temp = tempdir().unwrap();
        let inbox = temp.path().join("InBox___");
        fs::create_dir_all(&inbox).unwrap();
        fs::write(inbox.join("BoxInfo_____"), [0_u8; BOX_INFO_SIZE]).unwrap();

        let mut source_record = vec![0_u8; CEMU_RECORD_SLOT_SIZE];
        // Arena record conversion was added after 0.0.3, which gives this
        // fixture a stable historical signature without inventing a corrupt
        // container.
        let late_arena_record = 0x9B4 + 109 * 4;
        source_record[late_arena_record..late_arena_record + 4]
            .copy_from_slice(&[0x34, 0x12, 0x78, 0x56]);
        fs::write(
            inbox.join("_A"),
            received_message_with_record(&source_record),
        )
        .unwrap();

        let old =
            convert_cec_record_for_revision(&source_record, ConverterRevision::V0_0_3).unwrap();
        let latest = convert_cec_record(&source_record).unwrap();
        assert_ne!(old, latest);

        let mut current = empty_cemu_cec().unwrap();
        let occupied_slot = 7;
        let range = cemu_record_range(occupied_slot).unwrap();
        current[range.clone()].copy_from_slice(&old);
        // An unrelated current Cemu record must remain untouched.
        let unrelated_slot = 2;
        let unrelated_range = cemu_record_range(unrelated_slot).unwrap();
        current[unrelated_range.clone()].fill(0xA5);

        let repair = repair_cec_records(temp.path(), &current, None).unwrap();

        assert_eq!(repair.slots, vec![occupied_slot]);
        assert_eq!(&repair.bytes[range], latest.as_slice());
        assert_eq!(
            &repair.bytes[unrelated_range.clone()],
            &current[unrelated_range]
        );
        assert_eq!(repair.records.len(), 1);
    }

    #[test]
    fn transaction_file_create_error_identifies_the_actual_path() {
        let temp = tempdir().unwrap();
        let occupied_parent = temp.path().join("not-a-directory");
        fs::write(&occupied_parent, b"not a directory").unwrap();
        let path = occupied_parent.join(".cec.mh3g-tmp-test");

        let error = write_new_file(&path, b"converted cec").unwrap_err();

        let message = error.to_string();
        assert!(message.contains("I/O error while creating transaction file"));
        assert!(message.contains(path.to_str().unwrap()));
    }

    #[test]
    fn converts_packed_guild_card_scalars_before_cec_insertion() {
        let temp = tempdir().unwrap();
        let inbox = temp.path().join("InBox___");
        fs::create_dir_all(&inbox).unwrap();
        fs::write(inbox.join("BoxInfo_____"), [0_u8; BOX_INFO_SIZE]).unwrap();

        let slot_start = 0xE00;
        let rank_field = slot_start + 0x14;
        let weapon_usage_field = slot_start + 0x12C;
        let date_field = slot_start + 0x17A;
        let record_field = slot_start + 0x7C0 + 32 * 10;
        // Row 45 is intentionally beyond the sparse MEOW crown entries.  A
        // non-zero hunt count with no 3DS discovery bit must still become a
        // displayable Wii U Hunter's Notes row inside a packed CEC card.
        let late_record_field = slot_start + 0x7C0 + 45 * 10;
        let mut record = vec![0_u8; CEMU_RECORD_SLOT_SIZE];
        for equipment in 0_u8..5 {
            let equipment_field = slot_start + 0x4C + usize::from(equipment) * 0x10;
            record[equipment_field..equipment_field + 8].copy_from_slice(&[
                equipment + 1,
                0x70,
                0x20 + equipment,
                0x30,
                0xA0 + equipment,
                0xB0,
                0xC0 + equipment,
                0xD0,
            ]);
        }
        let tail_colors = slot_start + 0x110;
        record[tail_colors..tail_colors + 8]
            .copy_from_slice(&[0x12, 0x23, 0x34, 0x45, 0x56, 0x67, 0x78, 0x89]);
        record[rank_field..rank_field + 4].copy_from_slice(&[0x33, 0x00, 0xA1, 0xB2]);
        record[weapon_usage_field..weapon_usage_field + 8]
            .copy_from_slice(&[0x2B, 0x00, 0x1E, 0x00, 0x11, 0x00, 0x37, 0x00]);
        record[date_field..date_field + 2].copy_from_slice(&[0xEA, 0x07]);
        record[record_field..record_field + 10]
            .copy_from_slice(&[0x0F, 0x00, 0x10, 0x00, 0x64, 0x00, 0x65, 0x00, 0x03, 0x00]);
        record[late_record_field..late_record_field + 10]
            .copy_from_slice(&[0x09, 0x00, 0x00, 0x00, 0x64, 0x00, 0x64, 0x00, 0x00, 0x00]);

        let body_size = CEC_SOURCE_RECORD_PREFIX_SIZE + CEMU_RECORD_SLOT_SIZE;
        let header_size = MESSAGE_HEADER_SIZE;
        let mut message = vec![0_u8; header_size + body_size];
        let message_size = message.len() as u32;
        message[0..2].copy_from_slice(&0x6060_u16.to_le_bytes());
        message[4..8].copy_from_slice(&message_size.to_le_bytes());
        message[8..12].copy_from_slice(&(header_size as u32).to_le_bytes());
        message[12..16].copy_from_slice(&(body_size as u32).to_le_bytes());
        message[16..20].copy_from_slice(&MH3G_TITLE_ID.to_le_bytes());
        message[header_size + CEC_SOURCE_RECORD_PREFIX_SIZE..].copy_from_slice(&record);
        fs::write(inbox.join("_A"), message).unwrap();

        let target = empty_cemu_cec().unwrap();
        let conversion = convert_cec_records(temp.path(), &target, None).unwrap();
        let converted_rank = CEMU_HEADER_SIZE + CEMU_RECORD_AREA_OFFSET + rank_field;
        let converted_weapon_usage =
            CEMU_HEADER_SIZE + CEMU_RECORD_AREA_OFFSET + weapon_usage_field;
        let converted_date = CEMU_HEADER_SIZE + CEMU_RECORD_AREA_OFFSET + date_field;
        let converted_offset = CEMU_HEADER_SIZE + CEMU_RECORD_AREA_OFFSET + record_field;
        let converted_late_offset = CEMU_HEADER_SIZE + CEMU_RECORD_AREA_OFFSET + late_record_field;

        for equipment in 0_u8..5 {
            let equipment_field = slot_start + 0x4C + usize::from(equipment) * 0x10;
            let converted_equipment = CEMU_HEADER_SIZE + CEMU_RECORD_AREA_OFFSET + equipment_field;
            assert_eq!(
                &conversion.bytes[converted_equipment..converted_equipment + 8],
                &[
                    equipment + 1,
                    0x70,
                    0x30,
                    0x20 + equipment,
                    0xD0,
                    0xC0 + equipment,
                    0xB0,
                    0xA0 + equipment,
                ],
                "CEC equipment {equipment}"
            );
        }
        let converted_tail_colors = CEMU_HEADER_SIZE + CEMU_RECORD_AREA_OFFSET + tail_colors;
        assert_eq!(
            &conversion.bytes[converted_tail_colors..converted_tail_colors + 8],
            &[0x45, 0x34, 0x23, 0x12, 0x89, 0x78, 0x67, 0x56]
        );
        assert_eq!(
            &conversion.bytes[converted_rank..converted_rank + 4],
            &[0x00, 0x33, 0xA1, 0xB2]
        );
        assert_eq!(
            &conversion.bytes[converted_weapon_usage..converted_weapon_usage + 8],
            &[0x00, 0x2B, 0x00, 0x1E, 0x00, 0x11, 0x00, 0x37]
        );
        assert_eq!(
            &conversion.bytes[converted_date..converted_date + 2],
            &[0x07, 0xEA]
        );
        assert_eq!(
            &conversion.bytes[converted_offset..converted_offset + 10],
            &[0x00, 0x0F, 0x00, 0x10, 0x00, 0x64, 0x00, 0x65, 0xA0, 0x00]
        );
        assert_eq!(
            &conversion.bytes[converted_late_offset..converted_late_offset + 10],
            &[0x00, 0x09, 0x00, 0x00, 0x00, 0x64, 0x00, 0x64, 0x80, 0x00]
        );
    }

    #[test]
    fn converts_every_packed_guild_card_arena_record_before_cec_insertion() {
        let temp = tempdir().unwrap();
        let inbox = temp.path().join("InBox___");
        fs::create_dir_all(&inbox).unwrap();
        fs::write(inbox.join("BoxInfo_____"), [0_u8; BOX_INFO_SIZE]).unwrap();

        // CEC carries three packed 0xE00 guild-card slots. Pick the final
        // (110th) arena row of the final slot: it is the path rendered by
        // offline-hall partners.
        let arena_field = 2 * GUILD_CARD_SLOT_SIZE + 0x9B4 + 109 * 4;
        let arena_source = [0x81, 0x72, 0x63, 0x54];
        let following_field = arena_field + 4;
        let following_source = [0x11, 0x22, 0x33, 0x44];
        let mut record = vec![0_u8; CEMU_RECORD_SLOT_SIZE];
        record[arena_field..arena_field + 4].copy_from_slice(&arena_source);
        record[following_field..following_field + 4].copy_from_slice(&following_source);
        fs::write(inbox.join("_A"), received_message_with_record(&record)).unwrap();

        let conversion =
            convert_cec_records(temp.path(), &empty_cemu_cec().unwrap(), None).unwrap();
        let converted = CEMU_HEADER_SIZE + CEMU_RECORD_AREA_OFFSET + arena_field;
        assert_eq!(
            &conversion.bytes[converted..converted + 4],
            &u32::from_le_bytes(arena_source)
                .rotate_left(17)
                .to_be_bytes()
        );
        assert_eq!(
            &conversion.bytes[converted + 4..converted + 8],
            &following_source
        );
    }

    #[test]
    fn install_refuses_a_target_changed_after_conversion_planning() {
        let temp = tempdir().unwrap();
        let target = temp.path().join("cec");
        let initial = empty_cemu_cec().unwrap();
        fs::write(&target, &initial).unwrap();
        let mut planned = initial.clone();
        planned[CEMU_HEADER_SIZE + CEMU_RECORD_AREA_OFFSET] = 0xA5;
        let conversion = CecConversion {
            before_sha256: sha256_hex(&initial),
            after_sha256: sha256_hex(&planned),
            bytes: planned,
            records: Vec::new(),
            slots: Vec::new(),
            source_record_set_sha256: source_record_set_sha256(&[]),
        };

        let mut changed = initial;
        changed[CEMU_HEADER_SIZE + CEMU_RECORD_AREA_OFFSET] = 0x5A;
        fs::write(&target, changed).unwrap();

        let error = install_cec_with(temp.path(), &target, &conversion, &Stopped).unwrap_err();
        assert!(
            matches!(error, ConversionError::UnsafeInstall(message) if message.contains("changed"))
        );
        assert!(!temp.path().join(".cec.mh3g-install.json").exists());
    }

    fn planned_cec_conversion(before: &[u8], marker: u8) -> CecConversion {
        let mut bytes = before.to_vec();
        bytes[CEMU_HEADER_SIZE + CEMU_RECORD_AREA_OFFSET] = marker;
        CecConversion {
            before_sha256: sha256_hex(before),
            after_sha256: sha256_hex(&bytes),
            bytes,
            records: Vec::new(),
            slots: Vec::new(),
            source_record_set_sha256: source_record_set_sha256(&[]),
        }
    }

    #[test]
    fn cec_install_refuses_a_running_emulator_before_changing_the_target() {
        let temp = tempdir().unwrap();
        let target = temp.path().join("cec");
        let previous = empty_cemu_cec().unwrap();
        fs::write(&target, &previous).unwrap();
        let conversion = planned_cec_conversion(&previous, 0xA5);

        let error = install_cec_with(temp.path(), &target, &conversion, &Running).unwrap_err();

        assert!(
            matches!(error, ConversionError::UnsafeInstall(message) if message.contains("Cemu.exe"))
        );
        assert_eq!(fs::read(&target).unwrap(), previous);
        assert!(!temp.path().join(".cec.mh3g-install.json").exists());
    }

    #[test]
    fn cec_install_refuses_a_held_target_lock_before_reading_or_replacing_the_target() {
        let temp = tempdir().unwrap();
        let target = temp.path().join("cec");
        let previous = empty_cemu_cec().unwrap();
        fs::write(&target, &previous).unwrap();
        let conversion = planned_cec_conversion(&previous, 0xA5);
        let lock = temp.path().join(".cec.mh3g-install.lock");
        fs::write(&lock, b"held by another installer").unwrap();

        let error = install_cec_with(temp.path(), &target, &conversion, &Stopped).unwrap_err();

        assert!(
            matches!(error, ConversionError::UnsafeInstall(message) if message.contains("already locked"))
        );
        assert_eq!(fs::read(&target).unwrap(), previous);
        assert_eq!(fs::read(&lock).unwrap(), b"held by another installer");
        assert!(!temp.path().join(".cec.mh3g-install.json").exists());
    }

    #[test]
    fn cec_rollback_refuses_a_running_emulator_before_changing_the_target() {
        let temp = tempdir().unwrap();
        let target = temp.path().join("cec");
        let previous = empty_cemu_cec().unwrap();
        fs::write(&target, &previous).unwrap();
        let conversion = planned_cec_conversion(&previous, 0xA5);
        let installed = install_cec_with(temp.path(), &target, &conversion, &Stopped).unwrap();
        let current = fs::read(&target).unwrap();

        let error = rollback_cec_with(&installed.manifest, &Running).unwrap_err();

        assert!(
            matches!(error, ConversionError::UnsafeInstall(message) if message.contains("Cemu.exe"))
        );
        assert_eq!(fs::read(&target).unwrap(), current);
        assert!(installed.manifest.exists());
    }

    #[test]
    fn cec_rollback_refuses_a_held_target_lock_before_reading_or_replacing_the_target() {
        let temp = tempdir().unwrap();
        let target = temp.path().join("cec");
        let previous = empty_cemu_cec().unwrap();
        fs::write(&target, &previous).unwrap();
        let conversion = planned_cec_conversion(&previous, 0xA5);
        let installed = install_cec_with(temp.path(), &target, &conversion, &Stopped).unwrap();
        let current = fs::read(&target).unwrap();
        let lock = temp.path().join(".cec.mh3g-install.lock");
        fs::write(&lock, b"held by another rollback").unwrap();

        let error = rollback_cec_with(&installed.manifest, &Stopped).unwrap_err();

        assert!(
            matches!(error, ConversionError::UnsafeInstall(message) if message.contains("already locked"))
        );
        assert_eq!(fs::read(&target).unwrap(), current);
        assert_eq!(fs::read(&lock).unwrap(), b"held by another rollback");
        assert!(installed.manifest.exists());
    }

    #[cfg(unix)]
    #[test]
    fn install_failure_restores_target_without_removing_preexisting_manifest_entry() {
        use std::os::unix::fs::symlink;

        let temp = tempdir().unwrap();
        let target = temp.path().join("cec");
        let previous = empty_cemu_cec().unwrap();
        fs::write(&target, &previous).unwrap();
        let conversion = planned_cec_conversion(&previous, 0xA5);
        let previous_sha256 = sha256_hex(&previous);
        let backup = temp
            .path()
            .join(format!(".cec.mh3g-backup-{previous_sha256}"));
        let manifest = temp.path().join(".cec.mh3g-install.json");
        symlink(temp.path().join("missing-manifest-target"), &manifest).unwrap();

        install_cec_with(temp.path(), &target, &conversion, &Stopped).unwrap_err();

        assert_eq!(fs::read(&target).unwrap(), previous);
        assert!(
            fs::symlink_metadata(&manifest)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert!(!backup.exists());
    }

    #[cfg(unix)]
    #[test]
    fn install_refuses_an_existing_backup_symlink_before_changing_the_target() {
        use std::os::unix::fs::symlink;

        let temp = tempdir().unwrap();
        let target = temp.path().join("cec");
        let previous = empty_cemu_cec().unwrap();
        fs::write(&target, &previous).unwrap();
        let conversion = planned_cec_conversion(&previous, 0xA5);
        let backup = temp
            .path()
            .join(format!(".cec.mh3g-backup-{}", sha256_hex(&previous)));
        let linked_backup = temp.path().join("linked-backup");
        fs::write(&linked_backup, &previous).unwrap();
        symlink(&linked_backup, &backup).unwrap();

        install_cec_with(temp.path(), &target, &conversion, &Stopped).unwrap_err();

        assert_eq!(fs::read(&target).unwrap(), previous);
        assert!(
            fs::symlink_metadata(&backup)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert!(!temp.path().join(".cec.mh3g-install.json").exists());
    }

    #[test]
    fn rollback_cec_finishes_after_existing_target_was_already_restored() {
        let temp = tempdir().unwrap();
        let target = temp.path().join("cec");
        let previous = empty_cemu_cec().unwrap();
        fs::write(&target, &previous).unwrap();
        let conversion = planned_cec_conversion(&previous, 0xA5);
        let installed = install_cec_with(temp.path(), &target, &conversion, &Stopped).unwrap();
        let backup = installed.backup.unwrap();

        fs::write(&target, &previous).unwrap();
        fs::remove_file(&backup).unwrap();

        rollback_cec_with(&installed.manifest, &Stopped).unwrap();

        assert_eq!(fs::read(&target).unwrap(), previous);
        assert!(!backup.exists());
        assert!(!installed.manifest.exists());
    }

    #[test]
    fn rollback_cec_finishes_after_new_target_was_already_removed() {
        let temp = tempdir().unwrap();
        let target = temp.path().join("cec");
        let empty = empty_cemu_cec().unwrap();
        let conversion = planned_cec_conversion(&empty, 0xA5);
        let installed = install_cec_with(temp.path(), &target, &conversion, &Stopped).unwrap();

        fs::remove_file(&target).unwrap();

        rollback_cec_with(&installed.manifest, &Stopped).unwrap();

        assert!(!target.exists());
        assert!(!installed.manifest.exists());
    }
}
