use std::{
    collections::BTreeSet,
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::{
    ConversionError,
    profile::build_jp_cemu_header,
    transaction::{MacOsProcessProbe, ProcessProbe, sha256_hex},
};

const BOX_INFO_SIZE: usize = 0x20;
const MESSAGE_HEADER_SIZE: usize = 0x70;
pub const CEMU_HEADER_SIZE: usize = 40;
pub const CEMU_CEC_PAYLOAD_SIZE: usize = 0x835FC;
pub const CEMU_RECORD_AREA_OFFSET: usize = 0x1FC;
pub const CEMU_RECORD_SLOT_SIZE: usize = 0x2A00;
pub const CEMU_RECORD_SLOT_COUNT: usize = 50;
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
    pub before_sha256: String,
    pub after_sha256: String,
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
    let info_bytes = fs::read(&info_path)?;
    let parsed = parse_box_info(&info_bytes);
    let mut message_files = fs::read_dir(directory)?
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
    let bytes = fs::read(path)?;
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

    let slot_bytes = source_slot.map(fs::read).transpose()?;
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
    let mut paths = fs::read_dir(directory)?
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
    let bytes = fs::read(path)?;
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

/// Collect MH3G StreetPass records from both CEC boxes.
///
/// The 3DS message wrapper is not copied into Cemu. The observed MH3G message
/// body has an 8-byte prefix followed by one fixed 0x2A00-byte record, which is
/// the same slot size used by Cemu's 50-record cache.
pub fn collect_mh3g_records(root: &Path) -> Result<Vec<CecRecordSource>, ConversionError> {
    if !root.is_dir() {
        return Err(ConversionError::InvalidSave(format!(
            "3DS CEC mailbox is not a directory: {}",
            root.display()
        )));
    }
    let mut records = Vec::new();
    let mut seen = BTreeSet::new();
    for box_name in ["InBox___", "OutBox__"] {
        let directory = root.join(box_name);
        if !directory.join("BoxInfo_____").is_file() {
            return Err(ConversionError::InvalidSave(format!(
                "3DS CEC mailbox is missing BoxInfo_____: {}",
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
    }
    Ok(records)
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
    let records = collect_mh3g_records(source_dir)?;
    if records.is_empty() {
        return Err(ConversionError::InvalidSave(
            "3DS CEC mailbox contains no non-empty MH3G records".to_owned(),
        ));
    }

    let mut output = target_bytes.to_vec();
    let before_sha256 = sha256_hex(target_bytes);
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
        before_sha256,
    })
}

fn sync_directory(directory: &Path) -> Result<(), ConversionError> {
    File::open(directory)?.sync_all()?;
    Ok(())
}

fn write_new_file(path: &Path, bytes: &[u8]) -> Result<(), ConversionError> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}

/// Atomically install a converted Cemu cec cache, retaining a hash-addressed
/// backup and a manifest next to the target.
pub fn install_cec(
    source_dir: &Path,
    target: &Path,
    conversion: &CecConversion,
) -> Result<CecInstallResult, ConversionError> {
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
    if target
        .symlink_metadata()
        .map(|metadata| metadata.file_type().is_symlink())
        .unwrap_or(false)
    {
        return Err(ConversionError::InvalidSave(
            "CEC target cannot be a symlink".to_owned(),
        ));
    }
    if let Some(name) = MacOsProcessProbe.matching_process()? {
        return Err(ConversionError::UnsafeInstall(format!(
            "emulator process is running: {name}"
        )));
    }
    let manifest_path = parent.join(".cec.mh3g-install.json");
    if manifest_path.exists() {
        return Err(ConversionError::UnsafeInstall(format!(
            "CEC install manifest already exists: {}",
            manifest_path.display()
        )));
    }

    let previous = match fs::read(target) {
        Ok(bytes) => {
            validate_cemu_cec_bytes(&bytes)?;
            Some(bytes)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(error.into()),
    };
    let previous_sha256 = previous.as_deref().map(sha256_hex);
    let observed_sha256 = match previous_sha256.as_deref() {
        Some(hash) => hash.to_owned(),
        None => sha256_hex(&empty_cemu_cec()?),
    };
    if observed_sha256 != conversion.before_sha256 {
        return Err(ConversionError::UnsafeInstall(
            "CEC target changed after the conversion plan was created".to_owned(),
        ));
    }
    let backup = previous_sha256
        .as_deref()
        .map(|hash| parent.join(format!(".cec.mh3g-backup-{hash}")));
    if let (Some(path), Some(bytes)) = (&backup, previous.as_deref()) {
        match fs::read(path) {
            Ok(existing) if existing == bytes => {}
            Ok(_) => {
                return Err(ConversionError::UnsafeInstall(format!(
                    "CEC backup path already contains different bytes: {}",
                    path.display()
                )));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                write_new_file(path, bytes)?;
            }
            Err(error) => return Err(error.into()),
        }
    }

    let temporary = parent.join(format!(".cec.mh3g-tmp-{}", std::process::id()));
    write_new_file(&temporary, &conversion.bytes)?;
    let result = (|| {
        fs::rename(&temporary, target)?;
        sync_directory(parent)?;
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
        write_new_file(&manifest_path, &manifest_bytes)?;
        sync_directory(parent)?;
        Ok(CecInstallResult {
            backup,
            manifest: manifest_path.clone(),
            installed_sha256: conversion.after_sha256.clone(),
        })
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
        if let Some(previous) = previous.as_deref() {
            let restore = parent.join(format!(".cec.mh3g-restore-{}", std::process::id()));
            if write_new_file(&restore, previous).is_ok() {
                let _ = fs::rename(&restore, target);
            }
        } else {
            let _ = fs::remove_file(target);
        }
        let _ = fs::remove_file(&manifest_path);
    }
    result
}

/// Roll back a prior `install_cec` transaction after verifying the installed
/// hash and the hash-addressed backup.
pub fn rollback_cec(manifest_path: &Path) -> Result<(), ConversionError> {
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
    let manifest: CecInstallManifest = serde_json::from_slice(&fs::read(manifest_path)?)?;
    if manifest.version != 1 {
        return Err(ConversionError::InvalidSave(format!(
            "unsupported CEC install manifest version: {}",
            manifest.version
        )));
    }
    if manifest.target.file_name().and_then(|name| name.to_str()) != Some("cec")
        || manifest.target.parent() != Some(parent)
    {
        return Err(ConversionError::InvalidSave(
            "CEC rollback target is not bound to the manifest directory".to_owned(),
        ));
    }
    if manifest
        .target
        .symlink_metadata()
        .map(|metadata| metadata.file_type().is_symlink())
        .unwrap_or(false)
    {
        return Err(ConversionError::InvalidSave(
            "CEC rollback target cannot be a symlink".to_owned(),
        ));
    }
    if let Some(name) = MacOsProcessProbe.matching_process()? {
        return Err(ConversionError::UnsafeInstall(format!(
            "emulator process is running: {name}"
        )));
    }

    let current = match fs::read(&manifest.target) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(ConversionError::InvalidSave(
                "CEC rollback target is missing".to_owned(),
            ));
        }
        Err(error) => return Err(error.into()),
    };
    if sha256_hex(&current) != manifest.installed_sha256 {
        return Err(ConversionError::InvalidSave(
            "CEC rollback target hash does not match the install manifest".to_owned(),
        ));
    }

    match (&manifest.backup, &manifest.previous_sha256) {
        (Some(backup), Some(previous_sha256)) => {
            let expected = parent.join(format!(".cec.mh3g-backup-{previous_sha256}"));
            if backup != &expected {
                return Err(ConversionError::InvalidSave(
                    "CEC rollback backup path is not hash-bound".to_owned(),
                ));
            }
            let previous = fs::read(backup)?;
            if sha256_hex(&previous) != *previous_sha256 {
                return Err(ConversionError::InvalidSave(
                    "CEC rollback backup hash does not match the manifest".to_owned(),
                ));
            }
            let temporary = parent.join(format!(".cec.mh3g-rollback-{}", std::process::id()));
            write_new_file(&temporary, &previous)?;
            fs::rename(&temporary, &manifest.target)?;
            fs::remove_file(backup)?;
        }
        (None, None) => fs::remove_file(&manifest.target)?,
        _ => {
            return Err(ConversionError::InvalidSave(
                "CEC rollback manifest backup fields are inconsistent".to_owned(),
            ));
        }
    }
    fs::remove_file(manifest_path)?;
    sync_directory(parent)?;
    Ok(())
}

fn target_report(path: &Path) -> Result<CemuCecReport, ConversionError> {
    let bytes = fs::read(path)?;
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

    #[test]
    fn cemu_record_geometry_matches_native_japanese_container() {
        assert_eq!(
            CEMU_CEC_PAYLOAD_SIZE,
            CEMU_RECORD_AREA_OFFSET + CEMU_RECORD_SLOT_COUNT * CEMU_RECORD_SLOT_SIZE
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
        };

        let mut changed = initial;
        changed[CEMU_HEADER_SIZE + CEMU_RECORD_AREA_OFFSET] = 0x5A;
        fs::write(&target, changed).unwrap();

        let error = install_cec(temp.path(), &target, &conversion).unwrap_err();
        assert!(
            matches!(error, ConversionError::UnsafeInstall(message) if message.contains("changed"))
        );
        assert!(!temp.path().join(".cec.mh3g-install.json").exists());
    }
}
