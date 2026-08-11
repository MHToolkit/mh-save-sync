use std::{
    collections::BTreeMap,
    fs,
    fs::OpenOptions,
    io::Write,
    path::{Path, PathBuf},
    process,
};

use clap::{Parser, Subcommand};
use mh3g_save_convert::{
    ConversionError,
    cec::{
        CecInstallExpectations, convert_cec_records, empty_cemu_cec, inspect_cec,
        install_cec_from_source_with_expectations, rollback_cec,
    },
    compatibility::{
        CompatibilityMerge, DetectionConfidence, RevisionDetection, combine_revision_detections,
        detect_component_revision, merge_component,
    },
    converter::{
        EXTERNAL_COMPONENT_NAMES, SYSTEM_GALLERY_PAYLOAD_RANGE,
        convert_external_component_to_cemu_named, convert_source_to_cemu,
        merge_3ds_system_gallery_into_cemu_named, reset_guild_card_component_to_cemu_named,
    },
    events::event_snapshot,
    extras_transaction::{
        ExtraGroup, ExtraInstallEntry, ExtraInstallManifest, dry_run_extra_groups,
        install_extra_groups, rollback_extra_groups,
    },
    io_at_path,
    profile::{SaveProfile, inspect_bytes, validate_slot_path, validate_system_path},
    progress::quest_progress,
    revision::ConverterRevision,
    transaction::{
        InstallExpectations, existing_target_sha256, install_compatibility_merge_with_expectations,
        install_merged_component_with_expectations, install_with_expectations,
        manifest_path_for_target, rollback, sha256_hex,
    },
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use uuid::Uuid;

#[derive(Debug, Parser)]
#[command(name = "mh3g-save-convert", version)]
#[command(about = "Convert Japanese MH3G 3DS save data to Cemu")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Inspect a Japanese MH3G save without changing it.
    Inspect { source: PathBuf },
    /// Decode per-quest completion state, optionally comparing another slot.
    InspectProgress {
        source: PathBuf,
        #[arg(long)]
        target: Option<PathBuf>,
        #[arg(long)]
        quest_id: Option<u16>,
    },
    /// Decode story/event bitsets, optionally comparing another slot.
    InspectEvents {
        source: PathBuf,
        #[arg(long)]
        target: Option<PathBuf>,
        /// Include unset event coordinates as well as active ones.
        #[arg(long)]
        all: bool,
    },
    /// Inspect 3DS CEC/StreetPass messages and an optional Cemu cec cache.
    InspectCec {
        /// 3DS CEC mailbox directory (usually .../CEC/00048100).
        #[arg(long)]
        source_dir: PathBuf,
        /// Cemu MH3G `cec` file to inspect, without modifying it.
        #[arg(long)]
        target: Option<PathBuf>,
        /// Optional 3DS `user#` slot used to locate its guild-card anchor in CEC messages.
        #[arg(long)]
        source_slot: Option<PathBuf>,
    },
    /// Experimentally import raw received MH3G StreetPass/CEC records from InBox___ into a Cemu `cec` cache.
    ConvertCec {
        /// 3DS CEC mailbox directory (usually .../CEC/00048100).
        #[arg(long)]
        source_dir: PathBuf,
        /// Cemu MH3G `cec` cache. A missing file is initialized with the
        /// observed Japanese Cemu container header.
        #[arg(long)]
        target: PathBuf,
        /// Optional first Cemu slot to use; subsequent received records use following
        /// empty slots. Existing non-empty slots are never overwritten.
        #[arg(long)]
        slot: Option<usize>,
        /// Require the complete received-record-set SHA-256 observed during the preceding Dry Run.
        #[arg(long, requires = "write")]
        expected_source_record_set_sha256: Option<String>,
        /// Require the Cemu `cec` SHA-256 observed during the preceding Dry Run.
        /// A missing target is represented by the canonical empty Cemu container.
        #[arg(long, requires = "write")]
        expected_target_sha256: Option<String>,
        #[arg(long, conflicts_with = "write")]
        dry_run: bool,
        #[arg(long, conflicts_with = "dry_run", requires = "experimental")]
        write: bool,
        /// Acknowledge that raw CEC slot placement has file-level evidence only
        /// and has not been verified in the Wii U guild-card UI.
        #[arg(long)]
        experimental: bool,
    },
    /// Roll back a prior CEC import from its `.cec.mh3g-install.json` manifest.
    RollbackCec {
        #[arg(long)]
        manifest: PathBuf,
    },
    /// Convert a Japanese MH3G 3DS slot, dry-running unless --write is given.
    Convert {
        source: PathBuf,
        #[arg(long)]
        output: PathBuf,
        /// Require the source SHA-256 observed during the preceding Dry Run.
        #[arg(long, requires = "write")]
        expected_source_sha256: Option<String>,
        /// Require the target SHA-256 observed during the preceding Dry Run.
        #[arg(long, requires = "write")]
        expected_target_sha256: Option<String>,
        /// Require the target to remain absent from the preceding Dry Run until
        /// the transactional write acquires its per-slot lock.
        #[arg(long, requires = "write", conflicts_with = "expected_target_sha256")]
        expected_target_absent: bool,
        #[arg(long, conflicts_with = "write")]
        dry_run: bool,
        #[arg(long, conflicts_with = "dry_run")]
        write: bool,
    },
    /// Repair an older converted save without replacing later Wii U progress.
    RepairConverted {
        /// Original Japanese 3DS user1/user2/user3 used for the old conversion.
        source: PathBuf,
        /// Current same-numbered Wii U/Cemu slot after continued play.
        #[arg(long)]
        current: PathBuf,
        /// Destination Wii U/Cemu slot for the repaired result. When omitted,
        /// the legacy in-place behavior writes back to --current.
        #[arg(long)]
        output: Option<PathBuf>,
        /// Optional complete 3DS ExtData `user` directory. The current Cemu
        /// directory must contain all card*/quest* components; a separate
        /// output directory must already contain card1/card2/card3/cardbox.
        #[arg(long)]
        source_extdata_dir: Option<PathBuf>,
        /// Override automatic historical-version classification.
        #[arg(long, value_enum)]
        from_version: Option<ConverterRevision>,
        /// Require the complete original 3DS input-set SHA-256 from Dry Run.
        #[arg(long, requires = "write")]
        expected_source_set_sha256: Option<String>,
        /// Require the complete current Cemu input-set SHA-256 from Dry Run.
        #[arg(long, requires = "write")]
        expected_current_set_sha256: Option<String>,
        /// Require the selected output-state SHA-256 from Dry Run.
        #[arg(long, requires = "write")]
        expected_output_set_sha256: Option<String>,
        /// Require the exact merge-preview SHA-256 from Dry Run.
        #[arg(long, requires = "write")]
        expected_preview_sha256: Option<String>,
        #[arg(long, conflicts_with = "write")]
        dry_run: bool,
        #[arg(long, conflicts_with = "dry_run")]
        write: bool,
    },
    /// Roll back a compatibility repair as one coordinated transaction.
    RollbackRepair {
        #[arg(long)]
        manifest: PathBuf,
    },
    /// Merge 3DS gallery/movie flags into an existing Wii U/Cemu system file.
    ConvertSystem {
        source: PathBuf,
        #[arg(long)]
        output: PathBuf,
        /// Require the source SHA-256 observed during the preceding Dry Run.
        #[arg(long, requires = "write")]
        expected_source_sha256: Option<String>,
        /// Require the existing Wii U/Cemu target SHA-256 from the preceding Dry Run.
        #[arg(long, requires = "write")]
        expected_target_sha256: Option<String>,
        #[arg(long, conflicts_with = "write")]
        dry_run: bool,
        #[arg(long, conflicts_with = "dry_run")]
        write: bool,
    },
    /// Package MH3G 3DS extdata for Cemu without overwriting a save.
    ConvertExtras {
        /// 3DS MH3G extdata user directory (usually .../extdata/00000000/00000481/user).
        #[arg(long)]
        source_dir: PathBuf,
        /// New output directory for Cemu card*/quest* files; existing component files are refused.
        #[arg(long)]
        output_dir: PathBuf,
        #[arg(long, conflicts_with = "write")]
        dry_run: bool,
        #[arg(long, conflicts_with = "dry_run")]
        write: bool,
        /// Replace non-empty 3DS guild-card data with valid empty Cemu components.
        #[arg(long)]
        reset_guild_cards: bool,
    },
    /// Install one or more complete staged ExtData groups into an initialized Cemu target.
    InstallExtras {
        /// Directory containing all staged Cemu card*/quest* outputs.
        #[arg(long)]
        staging_dir: PathBuf,
        /// Initialized Cemu MH3G save directory containing the selected component groups.
        #[arg(long)]
        target_dir: PathBuf,
        /// Complete ExtData group(s) to install, for example guild-cards,quests.
        #[arg(long, value_enum, value_delimiter = ',', num_args = 1.., required = true)]
        groups: Vec<ExtraGroup>,
        /// Require the staged group fingerprint observed during dry-run.
        #[arg(long)]
        expected_staging_set_sha256: Option<String>,
        /// Require the target group fingerprint observed during dry-run.
        #[arg(long)]
        expected_target_set_sha256: Option<String>,
        #[arg(long, conflicts_with = "write")]
        dry_run: bool,
        #[arg(long, conflicts_with = "dry_run")]
        write: bool,
    },
    /// Roll back a complete ExtData transaction from its retained recovery journal.
    RollbackExtras {
        #[arg(long)]
        manifest: PathBuf,
    },
    /// Restore a save slot from a prior installation manifest.
    Rollback {
        #[arg(long)]
        manifest: PathBuf,
    },
}

#[derive(Debug, Serialize)]
struct Report {
    profile: Option<SaveProfile>,
    size: Option<usize>,
    hashes: BTreeMap<String, String>,
    output: Option<PathBuf>,
    backup: Option<PathBuf>,
    manifest: Option<PathBuf>,
    status: &'static str,
}

#[derive(Debug, Default)]
struct HashPreconditions {
    source_sha256: Option<String>,
    target_sha256: Option<String>,
    target_must_be_absent: bool,
}

#[derive(Debug)]
struct CecConversionOptions {
    expected_source_record_set_sha256: Option<String>,
    expected_target_sha256: Option<String>,
    dry_run: bool,
    write: bool,
    experimental: bool,
}

#[derive(Debug)]
struct RepairWriteOptions {
    expected_source_set_sha256: Option<String>,
    expected_current_set_sha256: Option<String>,
    expected_output_set_sha256: Option<String>,
    expected_preview_sha256: Option<String>,
    dry_run: bool,
    write: bool,
}

#[derive(Debug, Clone, Copy)]
struct ComponentConversionProfile {
    validate_path: fn(&Path) -> Result<(), ConversionError>,
    source: SaveProfile,
    output: SaveProfile,
}

#[derive(Debug, Serialize)]
struct QuestProgressComparison {
    table_index: usize,
    source_table_index: usize,
    target_table_index: usize,
    quest_id: u16,
    file: String,
    title_en: Option<String>,
    objective_en: Option<String>,
    area: String,
    star: Option<u8>,
    urgent: bool,
    key: Option<bool>,
    kind: String,
    completion_word: usize,
    completion_bit: u8,
    source_completed: bool,
    target_completed: Option<bool>,
    matches: Option<bool>,
}

#[derive(Debug, Serialize)]
struct ProgressReport {
    source: PathBuf,
    source_profile: SaveProfile,
    source_sha256: String,
    target: Option<PathBuf>,
    target_profile: Option<SaveProfile>,
    target_sha256: Option<String>,
    quests: Vec<QuestProgressComparison>,
    status: &'static str,
}

#[derive(Debug, Serialize)]
struct SimpleEventComparison {
    event_id: u16,
    domain: Option<String>,
    semantic_hint: Option<String>,
    three_ds_call_sites: Vec<String>,
    wiiu_call_sites: Vec<String>,
    source_set: bool,
    target_set: Option<bool>,
    matches: Option<bool>,
}

#[derive(Debug, Serialize)]
struct CategorizedEventComparison {
    category: u8,
    offset: u16,
    bit: u8,
    source_set: bool,
    target_set: Option<bool>,
    matches: Option<bool>,
}

#[derive(Debug, Serialize)]
struct EventReport {
    source: PathBuf,
    source_profile: SaveProfile,
    source_sha256: String,
    target: Option<PathBuf>,
    target_profile: Option<SaveProfile>,
    target_sha256: Option<String>,
    simple_events: Vec<SimpleEventComparison>,
    categorized_events: Vec<CategorizedEventComparison>,
    status: &'static str,
}

#[derive(Debug, Serialize)]
struct ExtraComponentReport {
    component: String,
    source_sha256: String,
    output_sha256: String,
    output: PathBuf,
    size: usize,
}

#[derive(Debug, Serialize)]
struct ExtrasReport {
    source_dir: PathBuf,
    output_dir: PathBuf,
    components: Vec<ExtraComponentReport>,
    status: &'static str,
}

#[derive(Debug, Serialize)]
struct ExtraInstallCliReport {
    operation: &'static str,
    status: &'static str,
    groups: Vec<ExtraGroup>,
    entries: Vec<ExtraInstallEntry>,
    manifest: PathBuf,
    staging_dir: PathBuf,
    target_dir: PathBuf,
    staging_set_sha256: String,
    target_set_sha256_before: String,
    backup_paths: Vec<PathBuf>,
}

#[derive(Debug, Serialize)]
struct ExtraRollbackCliReport {
    operation: &'static str,
    status: &'static str,
    groups: Vec<ExtraGroup>,
    entries: Vec<ExtraInstallEntry>,
    manifest: PathBuf,
}

#[derive(Debug, Serialize)]
struct CecConversionReport {
    source_dir: PathBuf,
    target: PathBuf,
    imported_messages: usize,
    source_record_sha256: Vec<String>,
    source_record_set_sha256: String,
    slots: Vec<usize>,
    target_sha256_before: String,
    target_sha256_after: String,
    backup: Option<PathBuf>,
    manifest: Option<PathBuf>,
    status: &'static str,
}

#[derive(Debug, Serialize)]
struct CecRollbackReport {
    manifest: PathBuf,
    status: &'static str,
}

#[derive(Debug, Serialize)]
struct RepairComponentReport {
    component: String,
    detection: RevisionDetection,
    merge: CompatibilityMerge,
    target: PathBuf,
    target_sha256_before: Option<String>,
    modified: bool,
    write_required: bool,
}

struct RepairComponentInput {
    component: String,
    source: Vec<u8>,
    current: Vec<u8>,
    target: PathBuf,
    target_before: Option<Vec<u8>>,
    detection: RevisionDetection,
}

#[derive(Debug, Serialize)]
struct RepairConvertedReport {
    operation: &'static str,
    status: &'static str,
    source: PathBuf,
    current: PathBuf,
    output: PathBuf,
    source_extdata_dir: Option<PathBuf>,
    source_set_sha256: String,
    current_set_sha256: String,
    output_set_sha256: String,
    preview_sha256: String,
    detection: RevisionDetection,
    components: Vec<RepairComponentReport>,
    preserved_components: Vec<String>,
    manifests: Vec<PathBuf>,
    compatibility_manifest: Option<PathBuf>,
}

const COMPATIBILITY_REPAIR_MANIFEST_VERSION: u32 = 2;
const COMPATIBILITY_REPAIR_MANIFEST_PREFIX: &str = ".mh3g-compatibility-repair-";

#[derive(Debug, Serialize, serde::Deserialize)]
struct CompatibilityRepairManifest {
    version: u32,
    transaction_id: String,
    #[serde(alias = "current_dir")]
    output_dir: PathBuf,
    source_set_sha256: String,
    current_set_sha256: String,
    #[serde(default)]
    output_set_sha256: Option<String>,
    preview_sha256: String,
    core_manifest: Option<PathBuf>,
    extras_manifest: Option<PathBuf>,
}

#[derive(Debug, Serialize)]
struct CompatibilityRollbackReport {
    operation: &'static str,
    manifest: PathBuf,
    status: &'static str,
}

fn main() {
    if let Err(error) = run(Cli::parse()) {
        eprintln!("{error}");
        process::exit(1);
    }
}

fn run(cli: Cli) -> Result<(), ConversionError> {
    match cli.command {
        Command::InspectProgress {
            source,
            target,
            quest_id,
        } => println!(
            "{}",
            serde_json::to_string(&inspect_progress(source, target, quest_id)?)?
        ),
        Command::InspectEvents {
            source,
            target,
            all,
        } => println!(
            "{}",
            serde_json::to_string(&inspect_events(source, target, all)?)?
        ),
        Command::InspectCec {
            source_dir,
            target,
            source_slot,
        } => println!(
            "{}",
            serde_json::to_string(&inspect_cec(source_dir, target, source_slot)?)?
        ),
        Command::ConvertCec {
            source_dir,
            target,
            slot,
            expected_source_record_set_sha256,
            expected_target_sha256,
            dry_run,
            write,
            experimental,
        } => println!(
            "{}",
            serde_json::to_string(&convert_cec(
                source_dir,
                target,
                slot,
                CecConversionOptions {
                    expected_source_record_set_sha256,
                    expected_target_sha256,
                    dry_run,
                    write,
                    experimental,
                },
            )?)?
        ),
        Command::RollbackCec { manifest } => {
            rollback_cec(&manifest)?;
            println!(
                "{}",
                serde_json::to_string(&CecRollbackReport {
                    manifest,
                    status: "rolled-back",
                })?
            );
        }
        Command::ConvertExtras {
            source_dir,
            output_dir,
            dry_run,
            write,
            reset_guild_cards,
        } => println!(
            "{}",
            serde_json::to_string(&convert_extras(
                source_dir,
                output_dir,
                dry_run,
                write,
                reset_guild_cards,
            )?)?
        ),
        Command::InstallExtras {
            staging_dir,
            target_dir,
            groups,
            expected_staging_set_sha256,
            expected_target_set_sha256,
            dry_run,
            write,
        } => println!(
            "{}",
            serde_json::to_string(&install_extras(
                staging_dir,
                target_dir,
                groups,
                expected_staging_set_sha256,
                expected_target_set_sha256,
                dry_run,
                write,
            )?)?
        ),
        Command::RollbackExtras { manifest } => {
            println!("{}", serde_json::to_string(&rollback_extras(manifest)?)?)
        }
        Command::RepairConverted {
            source,
            current,
            output,
            source_extdata_dir,
            from_version,
            expected_source_set_sha256,
            expected_current_set_sha256,
            expected_output_set_sha256,
            expected_preview_sha256,
            dry_run,
            write,
        } => println!(
            "{}",
            serde_json::to_string(&repair_converted(
                source,
                current,
                output,
                source_extdata_dir,
                from_version,
                RepairWriteOptions {
                    expected_source_set_sha256,
                    expected_current_set_sha256,
                    expected_output_set_sha256,
                    expected_preview_sha256,
                    dry_run,
                    write,
                },
            )?)?
        ),
        Command::RollbackRepair { manifest } => {
            println!("{}", serde_json::to_string(&rollback_repair(manifest)?)?)
        }
        command => {
            let report = match command {
                Command::Inspect { source } => inspect(source)?,
                Command::Convert {
                    source,
                    output,
                    expected_source_sha256,
                    expected_target_sha256,
                    expected_target_absent,
                    dry_run,
                    write,
                } => convert(
                    source,
                    output,
                    expected_source_sha256,
                    expected_target_sha256,
                    expected_target_absent,
                    dry_run,
                    write,
                )?,
                Command::ConvertSystem {
                    source,
                    output,
                    expected_source_sha256,
                    expected_target_sha256,
                    dry_run,
                    write,
                } => convert_system(
                    source,
                    output,
                    expected_source_sha256,
                    expected_target_sha256,
                    dry_run,
                    write,
                )?,
                Command::Rollback { manifest } => rollback_save(manifest)?,
                Command::InspectProgress { .. }
                | Command::InspectEvents { .. }
                | Command::InspectCec { .. }
                | Command::ConvertCec { .. }
                | Command::RollbackCec { .. }
                | Command::ConvertExtras { .. }
                | Command::InstallExtras { .. }
                | Command::RollbackExtras { .. }
                | Command::RepairConverted { .. }
                | Command::RollbackRepair { .. } => unreachable!(),
            };
            println!("{}", serde_json::to_string(&report)?);
        }
    }
    Ok(())
}

fn convert_cec(
    source_dir: PathBuf,
    target: PathBuf,
    slot: Option<usize>,
    options: CecConversionOptions,
) -> Result<CecConversionReport, ConversionError> {
    debug_assert!(!(options.dry_run && options.write));
    if options.write && !options.experimental {
        return Err(ConversionError::UnsafeInstall(
            "raw CEC import requires --experimental acknowledgement".to_owned(),
        ));
    }
    if options.write && options.expected_source_record_set_sha256.is_none() {
        return Err(ConversionError::UnsafeInstall(
            "CEC write requires --expected-source-record-set-sha256 from the preceding Dry Run"
                .to_owned(),
        ));
    }
    if options.write && options.expected_target_sha256.is_none() {
        return Err(ConversionError::UnsafeInstall(
            "CEC write requires --expected-target-sha256 from the preceding Dry Run".to_owned(),
        ));
    }

    let (conversion, backup, manifest, status) = if options.write {
        let installed = install_cec_from_source_with_expectations(
            &source_dir,
            &target,
            slot,
            CecInstallExpectations {
                source_record_set_sha256: options.expected_source_record_set_sha256.as_deref(),
                target_sha256: options.expected_target_sha256.as_deref(),
            },
        )?;
        (
            installed.conversion,
            installed.install.backup,
            Some(installed.install.manifest),
            "written",
        )
    } else {
        let target_bytes = match fs::read(&target) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => empty_cemu_cec()?,
            Err(error) => {
                return io_at_path(Err(error), "reading Cemu CEC target", &target);
            }
        };
        (
            convert_cec_records(&source_dir, &target_bytes, slot)?,
            None,
            None,
            "dry-run",
        )
    };
    let source_record_sha256 = conversion
        .records
        .iter()
        .map(|record| record.sha256.clone())
        .collect::<Vec<_>>();
    Ok(CecConversionReport {
        source_dir: source_dir.clone(),
        target: target.clone(),
        imported_messages: conversion.records.len(),
        source_record_sha256,
        source_record_set_sha256: conversion.source_record_set_sha256,
        slots: conversion.slots.clone(),
        target_sha256_before: conversion.before_sha256.clone(),
        target_sha256_after: conversion.after_sha256.clone(),
        backup,
        manifest,
        status,
    })
}

fn inspect_progress(
    source: PathBuf,
    target: Option<PathBuf>,
    quest_id: Option<u16>,
) -> Result<ProgressReport, ConversionError> {
    validate_slot_path(&source)?;
    let source_bytes = read_file(&source, "reading source save")?;
    let source_inspection = inspect_bytes(&source_bytes)?;
    let source_progress = quest_progress(&source_bytes)?;

    let (target_inspection, target_progress) = if let Some(path) = target.as_ref() {
        validate_slot_path(path)?;
        let bytes = read_file(path, "reading comparison save")?;
        (Some(inspect_bytes(&bytes)?), Some(quest_progress(&bytes)?))
    } else {
        (None, None)
    };

    let mut quests = Vec::new();
    for (index, source_state) in source_progress.into_iter().enumerate() {
        if quest_id.is_some_and(|filter| filter != source_state.quest.quest_id) {
            continue;
        }
        let target_completed = target_progress
            .as_ref()
            .and_then(|states| states.get(index))
            .map(|state| state.completed);
        quests.push(QuestProgressComparison {
            table_index: source_state.quest.table_index,
            source_table_index: source_state.quest.source_table_index,
            target_table_index: source_state.quest.target_table_index,
            quest_id: source_state.quest.quest_id,
            file: source_state.quest.file,
            title_en: source_state.quest.title_en,
            objective_en: source_state.quest.objective_en,
            area: source_state.quest.area,
            star: source_state.quest.star,
            urgent: source_state.quest.urgent,
            key: source_state.quest.key,
            kind: source_state.quest.kind,
            completion_word: source_state.quest.completion_word,
            completion_bit: source_state.quest.completion_bit,
            source_completed: source_state.completed,
            target_completed,
            matches: target_completed.map(|target| target == source_state.completed),
        });
    }

    if quests.is_empty()
        && let Some(quest_id) = quest_id
    {
        return Err(ConversionError::InvalidSave(format!(
            "quest ID {quest_id} is not present in the MH3G completion table"
        )));
    }

    Ok(ProgressReport {
        source,
        source_profile: source_inspection.profile,
        source_sha256: source_inspection.sha256,
        target,
        target_profile: target_inspection
            .as_ref()
            .map(|inspection| inspection.profile),
        target_sha256: target_inspection.map(|inspection| inspection.sha256),
        quests,
        status: "inspected-progress",
    })
}

fn inspect_events(
    source: PathBuf,
    target: Option<PathBuf>,
    all: bool,
) -> Result<EventReport, ConversionError> {
    validate_slot_path(&source)?;
    let source_bytes = read_file(&source, "reading source save")?;
    let source_inspection = inspect_bytes(&source_bytes)?;
    let compare_all = all || target.is_some();
    let source_events = event_snapshot(&source_bytes, compare_all)?;

    let (target_inspection, target_events) = if let Some(path) = target.as_ref() {
        validate_slot_path(path)?;
        let bytes = read_file(path, "reading comparison save")?;
        (
            Some(inspect_bytes(&bytes)?),
            Some(event_snapshot(&bytes, true)?),
        )
    } else {
        (None, None)
    };

    let simple_events = source_events
        .simple
        .into_iter()
        .enumerate()
        .filter_map(|(index, source_event)| {
            let target_set = target_events
                .as_ref()
                .and_then(|events| events.simple.get(index))
                .map(|event| event.set);
            (all || source_event.set || target_set == Some(true)).then(|| SimpleEventComparison {
                event_id: source_event.event_id,
                domain: source_event.domain,
                semantic_hint: source_event.semantic_hint,
                three_ds_call_sites: source_event.three_ds_call_sites,
                wiiu_call_sites: source_event.wiiu_call_sites,
                source_set: source_event.set,
                target_set,
                matches: target_set.map(|target| target == source_event.set),
            })
        })
        .collect();

    let categorized_events = source_events
        .categorized
        .into_iter()
        .enumerate()
        .filter_map(|(index, source_event)| {
            let target_set = target_events
                .as_ref()
                .and_then(|events| events.categorized.get(index))
                .map(|event| event.set);
            (all || source_event.set || target_set == Some(true)).then(|| {
                CategorizedEventComparison {
                    category: source_event.category,
                    offset: source_event.offset,
                    bit: source_event.bit,
                    source_set: source_event.set,
                    target_set,
                    matches: target_set.map(|target| target == source_event.set),
                }
            })
        })
        .collect();

    Ok(EventReport {
        source,
        source_profile: source_inspection.profile,
        source_sha256: source_inspection.sha256,
        target,
        target_profile: target_inspection
            .as_ref()
            .map(|inspection| inspection.profile),
        target_sha256: target_inspection.map(|inspection| inspection.sha256),
        simple_events,
        categorized_events,
        status: "inspected-events",
    })
}

fn inspect(source: PathBuf) -> Result<Report, ConversionError> {
    let source_bytes = read_file(&source, "reading source save")?;
    let inspection = inspect_bytes(&source_bytes)?;
    Ok(Report {
        profile: Some(inspection.profile),
        size: Some(inspection.size),
        hashes: BTreeMap::from([("source".to_owned(), inspection.sha256)]),
        output: None,
        backup: None,
        manifest: None,
        status: "inspected",
    })
}

fn convert(
    source: PathBuf,
    output: PathBuf,
    expected_source_sha256: Option<String>,
    expected_target_sha256: Option<String>,
    expected_target_absent: bool,
    dry_run: bool,
    write: bool,
) -> Result<Report, ConversionError> {
    convert_component(
        source,
        output,
        HashPreconditions {
            source_sha256: expected_source_sha256,
            target_sha256: expected_target_sha256,
            target_must_be_absent: expected_target_absent,
        },
        dry_run,
        write,
        ComponentConversionProfile {
            validate_path: validate_slot_path,
            source: SaveProfile::JpThreeDs,
            output: SaveProfile::JpCemu,
        },
    )
}

fn convert_system(
    source: PathBuf,
    output: PathBuf,
    expected_source_sha256: Option<String>,
    expected_target_sha256: Option<String>,
    dry_run: bool,
    write: bool,
) -> Result<Report, ConversionError> {
    debug_assert!(!(dry_run && write));
    validate_system_path(&source)?;
    validate_system_path(&output)?;
    if source.file_name() != output.file_name() {
        return Err(ConversionError::InvalidSave(
            "source and output shared-system names must both be system".to_owned(),
        ));
    }
    if !output.exists() {
        return Err(ConversionError::InvalidSave(format!(
            "convert-system requires an existing initialized Wii U/Cemu system target so shared data from other slots can be preserved: {}",
            output.display()
        )));
    }
    if write && (expected_source_sha256.is_none() || expected_target_sha256.is_none()) {
        return Err(ConversionError::UnsafeInstall(
            "convert-system --write requires --expected-source-sha256 and --expected-target-sha256 from the immediately preceding Dry Run"
                .to_owned(),
        ));
    }

    let source_bytes = read_file(&source, "reading 3DS system source")?;
    let source_inspection = inspect_bytes(&source_bytes)?;
    if source_inspection.profile != SaveProfile::JpThreeDsSystem {
        return Err(ConversionError::InvalidSave(format!(
            "unexpected system source profile: {:?}; expected JpThreeDsSystem",
            source_inspection.profile
        )));
    }
    let target_bytes = read_file(&output, "reading Wii U/Cemu system target")?;
    let target_inspection = inspect_bytes(&target_bytes)?;
    if target_inspection.profile != SaveProfile::JpCemuSystem {
        return Err(ConversionError::InvalidSave(format!(
            "unexpected system target profile: {:?}; expected JpCemuSystem",
            target_inspection.profile
        )));
    }

    let filename = output
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| ConversionError::InvalidSave("target filename is invalid".to_owned()))?;
    let merged = merge_3ds_system_gallery_into_cemu_named(&source_bytes, &target_bytes, filename)?;
    let merged_inspection = inspect_bytes(&merged)?;
    let source_gallery_start = 4 + SYSTEM_GALLERY_PAYLOAD_RANGE.start;
    let source_gallery_end = 4 + SYSTEM_GALLERY_PAYLOAD_RANGE.end;
    let target_payload_start = target_bytes.len() - mh3g_save_convert::profile::SYSTEM_PAYLOAD_SIZE;
    let target_gallery_start = target_payload_start + SYSTEM_GALLERY_PAYLOAD_RANGE.start;
    let target_gallery_end = target_payload_start + SYSTEM_GALLERY_PAYLOAD_RANGE.end;

    let mut report = Report {
        profile: Some(merged_inspection.profile),
        size: Some(merged_inspection.size),
        hashes: BTreeMap::from([
            ("source".to_owned(), source_inspection.sha256),
            ("target_before".to_owned(), target_inspection.sha256),
            ("output".to_owned(), merged_inspection.sha256),
            (
                "source_gallery".to_owned(),
                sha256_hex(&source_bytes[source_gallery_start..source_gallery_end]),
            ),
            (
                "target_gallery_before".to_owned(),
                sha256_hex(&target_bytes[target_gallery_start..target_gallery_end]),
            ),
            (
                "output_gallery".to_owned(),
                sha256_hex(&merged[target_gallery_start..target_gallery_end]),
            ),
        ]),
        output: Some(output.clone()),
        backup: None,
        manifest: None,
        status: "dry-run",
    };

    if write {
        let manifest_path = manifest_path_for_target(&output)?;
        let manifest = install_merged_component_with_expectations(
            &source_bytes,
            &merged,
            &output,
            &manifest_path,
            InstallExpectations {
                source_sha256: expected_source_sha256.as_deref(),
                target_sha256: expected_target_sha256.as_deref(),
                target_must_be_absent: false,
            },
        )?;
        report.backup = manifest.backup;
        report.manifest = Some(manifest_path);
        report.status = "written";
    }

    Ok(report)
}

fn convert_component(
    source: PathBuf,
    output: PathBuf,
    expectations: HashPreconditions,
    dry_run: bool,
    write: bool,
    profile: ComponentConversionProfile,
) -> Result<Report, ConversionError> {
    debug_assert!(!(dry_run && write));
    (profile.validate_path)(&source)?;
    (profile.validate_path)(&output)?;
    if source.file_name() != output.file_name() {
        return Err(ConversionError::InvalidSave(format!(
            "source and output save component names must match: {} != {}",
            source
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("invalid"),
            output
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("invalid")
        )));
    }

    let source_bytes = read_file(&source, "reading source save")?;
    let source_inspection = inspect_bytes(&source_bytes)?;
    if source_inspection.profile != profile.source {
        return Err(ConversionError::InvalidSave(format!(
            "unexpected source profile: {:?}",
            source_inspection.profile
        )));
    }
    let filename = output
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| ConversionError::InvalidSave("target filename is invalid".to_owned()))?;
    let converted = convert_source_to_cemu(&source_bytes, filename)?;
    let converted_inspection = inspect_bytes(&converted)?;
    debug_assert_eq!(converted_inspection.profile, profile.output);
    // A Dry Run establishes the target fingerprint up front. A real write
    // must instead let the installer read it under the per-slot lock.
    let target_sha256_before = if write {
        None
    } else {
        existing_target_sha256(&output)?
    };

    let mut hashes = BTreeMap::from([
        ("source".to_owned(), source_inspection.sha256),
        ("output".to_owned(), converted_inspection.sha256),
    ]);
    if let Some(target_sha256_before) = target_sha256_before {
        hashes.insert("target_before".to_owned(), target_sha256_before);
    }

    let mut report = Report {
        profile: Some(converted_inspection.profile),
        size: Some(converted_inspection.size),
        hashes,
        output: Some(output.clone()),
        backup: None,
        manifest: None,
        status: "dry-run",
    };

    if write {
        let manifest_path = manifest_path_for_target(&output)?;
        let manifest = install_with_expectations(
            &source_bytes,
            &converted,
            &output,
            &manifest_path,
            InstallExpectations {
                source_sha256: expectations.source_sha256.as_deref(),
                target_sha256: expectations.target_sha256.as_deref(),
                target_must_be_absent: expectations.target_must_be_absent,
            },
        )?;
        if let Some(target_sha256_before) = manifest.previous_sha256.as_ref() {
            report
                .hashes
                .insert("target_before".to_owned(), target_sha256_before.clone());
        }
        report.backup = manifest.backup;
        report.manifest = Some(manifest_path);
        report.status = "written";
    } else {
        debug_assert!(dry_run || !write);
    }

    Ok(report)
}

fn read_file(path: &Path, operation: &'static str) -> Result<Vec<u8>, ConversionError> {
    io_at_path(fs::read(path), operation, path)
}

fn read_optional_file(
    path: &Path,
    operation: &'static str,
) -> Result<Option<Vec<u8>>, ConversionError> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => io_at_path(Err(error), operation, path),
    }
}

fn repair_converted(
    source: PathBuf,
    current: PathBuf,
    output: Option<PathBuf>,
    source_extdata_dir: Option<PathBuf>,
    from_version: Option<ConverterRevision>,
    options: RepairWriteOptions,
) -> Result<RepairConvertedReport, ConversionError> {
    debug_assert!(!(options.dry_run && options.write));
    validate_slot_path(&source)?;
    validate_slot_path(&current)?;
    let require_output_expectation = output.is_some();
    let output_selection = output.unwrap_or_else(|| current.clone());
    validate_slot_path(&output_selection)?;
    if source.file_name() != current.file_name()
        || source.file_name() != output_selection.file_name()
    {
        return Err(ConversionError::InvalidSave(format!(
            "original 3DS, current Cemu, and output slots must have the same basename: {}, {}, {}",
            source.display(),
            current.display(),
            output_selection.display()
        )));
    }
    let slot_name = source
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| ConversionError::InvalidSave("save slot name is invalid".to_owned()))?;
    let current_parent = current.parent().ok_or_else(|| {
        ConversionError::InvalidSave("current Cemu slot has no parent directory".to_owned())
    })?;
    let current_dir = io_at_path(
        fs::canonicalize(current_parent),
        "resolving compatibility current directory",
        current_parent,
    )?;
    let output_parent = output_selection.parent().ok_or_else(|| {
        ConversionError::InvalidSave("compatibility output slot has no parent directory".to_owned())
    })?;
    let output_dir = io_at_path(
        fs::canonicalize(output_parent),
        "resolving compatibility output directory",
        output_parent,
    )?;
    let output_path = output_dir.join(slot_name);

    let source_slot = read_file(&source, "reading original 3DS compatibility source")?;
    let current_slot = read_file(&current, "reading current Cemu compatibility target")?;
    let output_slot_before =
        read_optional_file(&output_path, "reading compatibility output target")?;
    if let Some(bytes) = output_slot_before.as_deref()
        && inspect_bytes(bytes)?.profile != SaveProfile::JpCemu
    {
        return Err(ConversionError::InvalidSave(format!(
            "compatibility output must be a Japanese Cemu slot or an absent user# path: {}",
            output_path.display()
        )));
    }
    let mut source_set = BTreeMap::from([(slot_name.to_owned(), source_slot.clone())]);
    let mut current_set = BTreeMap::from([(slot_name.to_owned(), current_slot.clone())]);
    let mut output_set = BTreeMap::from([(slot_name.to_owned(), output_slot_before.clone())]);

    let slot_detection = detect_component_revision(&source_slot, &current_slot, slot_name)?;
    let mut repair_inputs = vec![RepairComponentInput {
        component: slot_name.to_owned(),
        detection: slot_detection,
        source: source_slot.clone(),
        current: current_slot.clone(),
        target: output_path.clone(),
        target_before: output_slot_before,
    }];
    let mut preserved_components = Vec::new();

    if let Some(extdata_dir) = source_extdata_dir.as_deref() {
        if !extdata_dir.is_dir() {
            return Err(ConversionError::InvalidSave(format!(
                "3DS ExtData source is not a directory: {}",
                extdata_dir.display()
            )));
        }
        for component in EXTERNAL_COMPONENT_NAMES {
            let source_path = extdata_dir.join(component);
            let current_path = current_dir.join(component);
            if !source_path.is_file() || !current_path.is_file() {
                return Err(ConversionError::InvalidSave(format!(
                    "compatibility merge requires matching complete ExtData components: {} and {}",
                    source_path.display(),
                    current_path.display()
                )));
            }
            let source_bytes = read_file(&source_path, "reading original 3DS ExtData")?;
            let current_bytes = read_file(&current_path, "reading current Cemu ExtData")?;
            // This validates both profiles even for payload-preserving quest
            // components that are not changed by compatibility repair.
            let latest = convert_external_component_to_cemu_named(&source_bytes, component)?;
            mh3g_save_convert::converter::validate_cemu_external_component_named(
                &current_bytes,
                component,
            )?;
            debug_assert_eq!(latest.len(), current_bytes.len());
            source_set.insert(component.to_owned(), source_bytes.clone());
            current_set.insert(component.to_owned(), current_bytes.clone());

            if matches!(component, "card1" | "card2" | "card3" | "cardbox") {
                let output_path = output_dir.join(component);
                let output_bytes = read_optional_file(
                    &output_path,
                    "reading compatibility ExtData output target",
                )?
                .ok_or_else(|| {
                    ConversionError::InvalidSave(format!(
                        "compatibility output ExtData component is missing; choose an initialized Wii U/Cemu output directory: {}",
                        output_path.display()
                    ))
                })?;
                mh3g_save_convert::converter::validate_cemu_external_component_named(
                    &output_bytes,
                    component,
                )?;
                output_set.insert(component.to_owned(), Some(output_bytes.clone()));
                let detection =
                    detect_component_revision(&source_bytes, &current_bytes, component)?;
                repair_inputs.push(RepairComponentInput {
                    component: component.to_owned(),
                    detection,
                    source: source_bytes,
                    current: current_bytes,
                    target: output_path,
                    target_before: Some(output_bytes),
                });
            } else {
                preserved_components.push(component.to_owned());
            }
        }
    }

    let detection = combine_revision_detections(
        &repair_inputs
            .iter()
            .map(|input| input.detection.clone())
            .collect::<Vec<_>>(),
    );
    let revision = select_repair_revision(&detection, from_version, !options.write)?;
    let components = repair_inputs
        .into_iter()
        .map(|input| {
            let merge = merge_component(&input.source, &input.current, &input.component, revision)?;
            let target_sha256_before = input.target_before.as_deref().map(sha256_hex);
            let write_required = input
                .target_before
                .as_deref()
                .is_none_or(|before| before != merge.bytes);
            Ok(RepairComponentReport {
                component: input.component,
                detection: input.detection,
                modified: merge.current_sha256 != merge.merged_sha256,
                write_required,
                merge,
                target: input.target,
                target_sha256_before,
            })
        })
        .collect::<Result<Vec<_>, ConversionError>>()?;

    let source_set_sha256 = component_set_sha256(&source_set);
    let current_set_sha256 = component_set_sha256(&current_set);
    let output_set_sha256 = component_state_set_sha256(&output_set);
    let preview_bytes = serde_json::to_vec(&(
        &source_set_sha256,
        &current_set_sha256,
        &output_set_sha256,
        &detection,
        &components,
        &preserved_components,
    ))?;
    let preview_sha256 = hex::encode(Sha256::digest(preview_bytes));

    if options.write {
        require_repair_expectation(
            options.expected_source_set_sha256.as_deref(),
            &source_set_sha256,
            "source set",
        )?;
        require_repair_expectation(
            options.expected_current_set_sha256.as_deref(),
            &current_set_sha256,
            "current set",
        )?;
        if require_output_expectation {
            require_repair_expectation(
                options.expected_output_set_sha256.as_deref(),
                &output_set_sha256,
                "output set",
            )?;
        }
        require_repair_expectation(
            options.expected_preview_sha256.as_deref(),
            &preview_sha256,
            "preview",
        )?;
    }

    let mut manifests = Vec::new();
    let mut core_manifest = None;
    let mut extras_manifest = None;
    if options.write && components[0].write_required {
        let manifest_path = manifest_path_for_target(&output_path)?;
        install_compatibility_merge_with_expectations(
            &source_slot,
            &components[0].merge.bytes,
            &output_path,
            &manifest_path,
            InstallExpectations {
                source_sha256: Some(components[0].merge.source_sha256.as_str()),
                target_sha256: components[0].target_sha256_before.as_deref(),
                target_must_be_absent: components[0].target_sha256_before.is_none(),
            },
        )?;
        manifests.push(manifest_path.clone());
        core_manifest = Some(manifest_path);
    }

    let card_components = components
        .iter()
        .skip(1)
        .filter(|component| {
            matches!(
                component.component.as_str(),
                "card1" | "card2" | "card3" | "cardbox"
            )
        })
        .collect::<Vec<_>>();
    let cards_write_required = card_components
        .iter()
        .any(|component| component.write_required);
    if options.write && cards_write_required {
        let staging_parent = output_dir.parent().ok_or_else(|| {
            ConversionError::InvalidSave(
                "output Cemu save directory has no parent for compatibility staging".to_owned(),
            )
        })?;
        let staging_dir = staging_parent.join(format!(".mh3g-compat-staging-{}", Uuid::new_v4()));
        io_at_path(
            fs::create_dir(&staging_dir),
            "creating compatibility staging directory",
            &staging_dir,
        )?;
        let install_result = (|| {
            for component in EXTERNAL_COMPONENT_NAMES {
                let bytes = if let Some(merged) = card_components
                    .iter()
                    .find(|candidate| candidate.component == component)
                {
                    merged.merge.bytes.as_slice()
                } else {
                    current_set
                        .get(component)
                        .expect("complete current ExtData set was validated")
                        .as_slice()
                };
                let path = staging_dir.join(component);
                io_at_path(
                    fs::write(&path, bytes),
                    "writing compatibility staging component",
                    &path,
                )?;
            }
            let groups = [ExtraGroup::GuildCards];
            let dry_run = dry_run_extra_groups(&staging_dir, &output_dir, &groups, None, None)?;
            install_extra_groups(
                &staging_dir,
                &output_dir,
                &groups,
                Some(&dry_run.staging_set_sha256),
                Some(&dry_run.target_set_sha256),
            )
        })();
        let _ = fs::remove_dir_all(&staging_dir);
        match install_result {
            Ok(report) => {
                manifests.push(report.manifest_path.clone());
                extras_manifest = Some(report.manifest_path);
            }
            Err(error) => {
                if let Some(manifest) = core_manifest.as_deref()
                    && let Err(rollback_error) = rollback(manifest)
                {
                    return Err(ConversionError::UnsafeInstall(format!(
                        "guild-card compatibility install failed: {error}; core rollback also failed: {rollback_error}; retain {}",
                        manifest.display()
                    )));
                }
                return Err(error);
            }
        }
    }

    let any_write_required = components.iter().any(|component| component.write_required);
    let compatibility_manifest = if options.write && !manifests.is_empty() {
        let transaction_id = Uuid::new_v4().hyphenated().to_string();
        let manifest_path = output_dir.join(format!(
            "{COMPATIBILITY_REPAIR_MANIFEST_PREFIX}{transaction_id}.json"
        ));
        let manifest = CompatibilityRepairManifest {
            version: COMPATIBILITY_REPAIR_MANIFEST_VERSION,
            transaction_id,
            output_dir: output_dir.clone(),
            source_set_sha256: source_set_sha256.clone(),
            current_set_sha256: current_set_sha256.clone(),
            output_set_sha256: Some(output_set_sha256.clone()),
            preview_sha256: preview_sha256.clone(),
            core_manifest: core_manifest.clone(),
            extras_manifest: extras_manifest.clone(),
        };
        if let Err(error) = write_compatibility_manifest(&manifest_path, &manifest) {
            let mut rollback_errors = Vec::new();
            if let Some(extras) = extras_manifest.as_deref()
                && let Err(rollback_error) = rollback_extra_groups(extras)
            {
                rollback_errors.push(format!(
                    "guild-card rollback {}: {rollback_error}",
                    extras.display()
                ));
            }
            if let Some(core) = core_manifest.as_deref()
                && let Err(rollback_error) = rollback(core)
            {
                rollback_errors.push(format!(
                    "core rollback {}: {rollback_error}",
                    core.display()
                ));
            }
            if rollback_errors.is_empty() {
                return Err(error);
            }
            return Err(ConversionError::UnsafeInstall(format!(
                "compatibility repair succeeded but coordinator manifest publication failed: {error}; compensation also failed: {}",
                rollback_errors.join("; ")
            )));
        }
        Some(manifest_path)
    } else {
        None
    };
    Ok(RepairConvertedReport {
        operation: "repair-converted",
        status: if options.write {
            if any_write_required {
                "written"
            } else {
                "no-changes"
            }
        } else {
            "dry-run"
        },
        source,
        current,
        output: output_selection,
        source_extdata_dir,
        source_set_sha256,
        current_set_sha256,
        output_set_sha256,
        preview_sha256,
        detection,
        components,
        preserved_components,
        manifests,
        compatibility_manifest,
    })
}

fn write_compatibility_manifest(
    path: &Path,
    manifest: &CompatibilityRepairManifest,
) -> Result<(), ConversionError> {
    let bytes = serde_json::to_vec_pretty(manifest)?;
    let mut file = io_at_path(
        OpenOptions::new().write(true).create_new(true).open(path),
        "creating compatibility repair manifest",
        path,
    )?;
    io_at_path(
        file.write_all(&bytes).and_then(|_| file.sync_all()),
        "writing compatibility repair manifest",
        path,
    )
}

fn rollback_repair(manifest_path: PathBuf) -> Result<CompatibilityRollbackReport, ConversionError> {
    let manifest_path = io_at_path(
        fs::canonicalize(&manifest_path),
        "resolving compatibility repair manifest",
        &manifest_path,
    )?;
    let parent = manifest_path.parent().ok_or_else(|| {
        ConversionError::InvalidSave(
            "compatibility repair manifest has no parent directory".to_owned(),
        )
    })?;
    let filename = manifest_path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| {
            ConversionError::InvalidSave(
                "compatibility repair manifest filename is invalid".to_owned(),
            )
        })?;
    let transaction_id = filename
        .strip_prefix(COMPATIBILITY_REPAIR_MANIFEST_PREFIX)
        .and_then(|name| name.strip_suffix(".json"))
        .ok_or_else(|| {
            ConversionError::InvalidSave(
                "compatibility repair manifest filename is not controlled".to_owned(),
            )
        })?;
    let parsed_id = Uuid::parse_str(transaction_id).map_err(|_| {
        ConversionError::InvalidSave(
            "compatibility repair manifest transaction ID is invalid".to_owned(),
        )
    })?;
    if parsed_id.hyphenated().to_string() != transaction_id {
        return Err(ConversionError::InvalidSave(
            "compatibility repair manifest transaction ID is not canonical".to_owned(),
        ));
    }

    let bytes = read_file(&manifest_path, "reading compatibility repair manifest")?;
    let manifest: CompatibilityRepairManifest = serde_json::from_slice(&bytes)?;
    if !matches!(manifest.version, 1 | COMPATIBILITY_REPAIR_MANIFEST_VERSION)
        || manifest.transaction_id != transaction_id
        || manifest.output_dir != parent
        || manifest.core_manifest.is_none() && manifest.extras_manifest.is_none()
    {
        return Err(ConversionError::InvalidSave(
            "compatibility repair manifest metadata is inconsistent".to_owned(),
        ));
    }
    for (label, hash) in [
        ("source set", &manifest.source_set_sha256),
        ("current set", &manifest.current_set_sha256),
        ("preview", &manifest.preview_sha256),
    ] {
        if hash.len() != 64 || !hash.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(ConversionError::InvalidSave(format!(
                "compatibility repair {label} SHA-256 is invalid"
            )));
        }
    }
    match (manifest.version, manifest.output_set_sha256.as_deref()) {
        (1, None) => {}
        (COMPATIBILITY_REPAIR_MANIFEST_VERSION, Some(hash))
            if hash.len() == 64 && hash.bytes().all(|byte| byte.is_ascii_hexdigit()) => {}
        _ => {
            return Err(ConversionError::InvalidSave(
                "compatibility repair output set SHA-256 is invalid".to_owned(),
            ));
        }
    }
    for child in [
        manifest.core_manifest.as_deref(),
        manifest.extras_manifest.as_deref(),
    ]
    .into_iter()
    .flatten()
    {
        if !child.is_absolute() || !child.starts_with(parent) {
            return Err(ConversionError::InvalidSave(format!(
                "compatibility repair child manifest is outside its target directory: {}",
                child.display()
            )));
        }
    }

    if let Some(extras) = manifest.extras_manifest.as_deref() {
        rollback_extra_groups(extras)?;
    }
    if let Some(core) = manifest.core_manifest.as_deref() {
        rollback(core)?;
    }
    io_at_path(
        fs::remove_file(&manifest_path),
        "removing consumed compatibility repair manifest",
        &manifest_path,
    )?;
    Ok(CompatibilityRollbackReport {
        operation: "rollback-repair",
        manifest: manifest_path,
        status: "rolled-back",
    })
}

fn select_repair_revision(
    detection: &RevisionDetection,
    override_revision: Option<ConverterRevision>,
    allow_ambiguous_preview: bool,
) -> Result<ConverterRevision, ConversionError> {
    if let Some(revision) = override_revision {
        let supported = detection.scores.iter().any(|score| {
            score.revision == revision && (score.matching_fields > 0 || score.already_current > 0)
        });
        let has_discriminators = detection
            .scores
            .iter()
            .any(|score| score.matching_fields > 0 || score.already_current > 0);
        if !supported && has_discriminators {
            return Err(ConversionError::InvalidSave(format!(
                "requested converter revision {} is contradicted by the selected component",
                revision.label()
            )));
        }
        return Ok(revision);
    }

    match detection.confidence {
        DetectionConfidence::Exact | DetectionConfidence::CompatibleRange => detection
            .candidates
            .first()
            .copied()
            .ok_or_else(|| {
                ConversionError::InvalidSave(
                    "converter revision detection returned no candidate".to_owned(),
                )
            }),
        DetectionConfidence::Ambiguous if allow_ambiguous_preview => detection
            .candidates
            .first()
            .copied()
            .ok_or_else(|| {
                ConversionError::InvalidSave(
                    "ambiguous converter revision detection returned no candidate".to_owned(),
                )
            }),
        DetectionConfidence::Ambiguous => Err(ConversionError::InvalidSave(
            "historical converter revision is ambiguous; run Dry Run, inspect the candidates, and repeat with --from-version"
                .to_owned(),
        )),
        DetectionConfidence::Unknown => Err(ConversionError::InvalidSave(
            "current Wii U component does not match a supported 0.0.3-0.0.6 conversion"
                .to_owned(),
        )),
    }
}

fn component_set_sha256(components: &BTreeMap<String, Vec<u8>>) -> String {
    let mut digest = Sha256::new();
    digest.update(b"mh3g-compatibility-component-set-v1\0");
    for (name, bytes) in components {
        let content_sha256 = sha256_hex(bytes);
        digest.update((name.len() as u64).to_be_bytes());
        digest.update(name.as_bytes());
        digest.update(content_sha256.as_bytes());
    }
    hex::encode(digest.finalize())
}

fn component_state_set_sha256(components: &BTreeMap<String, Option<Vec<u8>>>) -> String {
    let mut digest = Sha256::new();
    digest.update(b"mh3g-compatibility-output-state-v1\0");
    for (name, bytes) in components {
        digest.update((name.len() as u64).to_be_bytes());
        digest.update(name.as_bytes());
        match bytes {
            Some(bytes) => {
                digest.update([1]);
                digest.update(sha256_hex(bytes).as_bytes());
            }
            None => digest.update([0]),
        }
    }
    hex::encode(digest.finalize())
}

fn require_repair_expectation(
    expected: Option<&str>,
    observed: &str,
    label: &str,
) -> Result<(), ConversionError> {
    let expected = expected.ok_or_else(|| {
        ConversionError::UnsafeInstall(format!(
            "compatibility write requires the expected {label} SHA-256 from Dry Run"
        ))
    })?;
    if expected != observed {
        return Err(ConversionError::UnsafeInstall(format!(
            "compatibility {label} SHA-256 changed after Dry Run: expected {expected}, observed {observed}"
        )));
    }
    Ok(())
}

fn convert_extras(
    source_dir: PathBuf,
    output_dir: PathBuf,
    dry_run: bool,
    write: bool,
    reset_guild_cards: bool,
) -> Result<ExtrasReport, ConversionError> {
    debug_assert!(!(dry_run && write));
    if !source_dir.is_dir() {
        return Err(ConversionError::InvalidSave(format!(
            "3DS MH3G extdata source is not a directory: {}",
            source_dir.display()
        )));
    }
    if output_dir.exists() && !output_dir.is_dir() {
        return Err(ConversionError::InvalidSave(format!(
            "Cemu extra-data output is not a directory: {}",
            output_dir.display()
        )));
    }

    let mut converted = Vec::with_capacity(EXTERNAL_COMPONENT_NAMES.len());
    for component in EXTERNAL_COMPONENT_NAMES {
        let source = source_dir.join(component);
        if !source.is_file() {
            return Err(ConversionError::InvalidSave(format!(
                "required 3DS MH3G extra-data component is missing: {}",
                source.display()
            )));
        }
        let source_bytes = read_file(&source, "reading 3DS extra-data component")?;
        let output_bytes =
            if reset_guild_cards && matches!(component, "card1" | "card2" | "card3" | "cardbox") {
                reset_guild_card_component_to_cemu_named(&source_bytes, component)?
            } else {
                convert_external_component_to_cemu_named(&source_bytes, component)?
            };
        converted.push((component, source_bytes, output_bytes));
    }

    let output_paths = converted
        .iter()
        .map(|(component, _, _)| output_dir.join(component))
        .collect::<Vec<_>>();
    if write && output_paths.iter().any(|path| path.exists()) {
        let occupied = output_paths
            .iter()
            .find(|path| path.exists())
            .expect("an occupied output path exists");
        return Err(ConversionError::UnsafeInstall(format!(
            "extra-data output already exists; use a new empty directory: {}",
            occupied.display()
        )));
    }

    let components = converted
        .iter()
        .zip(output_paths.iter())
        .map(
            |((component, source_bytes, output_bytes), output)| ExtraComponentReport {
                component: (*component).to_owned(),
                source_sha256: sha256_hex(source_bytes),
                output_sha256: sha256_hex(output_bytes),
                output: output.clone(),
                size: output_bytes.len(),
            },
        )
        .collect();

    if write {
        io_at_path(
            fs::create_dir_all(&output_dir),
            "creating extra-data output directory",
            &output_dir,
        )?;
        for ((_, _, output_bytes), output) in converted.iter().zip(output_paths.iter()) {
            io_at_path(
                fs::write(output, output_bytes),
                "writing converted extra-data component",
                output,
            )?;
        }
    } else {
        debug_assert!(dry_run || !write);
    }

    Ok(ExtrasReport {
        source_dir,
        output_dir,
        components,
        status: if write { "written" } else { "dry-run" },
    })
}

fn install_extras(
    staging_dir: PathBuf,
    target_dir: PathBuf,
    groups: Vec<ExtraGroup>,
    expected_staging_set_sha256: Option<String>,
    expected_target_set_sha256: Option<String>,
    dry_run: bool,
    write: bool,
) -> Result<ExtraInstallCliReport, ConversionError> {
    debug_assert!(!(dry_run && write));
    let report = if write {
        install_extra_groups(
            &staging_dir,
            &target_dir,
            &groups,
            expected_staging_set_sha256.as_deref(),
            expected_target_set_sha256.as_deref(),
        )?
    } else {
        dry_run_extra_groups(
            &staging_dir,
            &target_dir,
            &groups,
            expected_staging_set_sha256.as_deref(),
            expected_target_set_sha256.as_deref(),
        )?
    };
    let backup_paths = report
        .entries
        .iter()
        .filter_map(|entry| entry.backup.clone())
        .collect();
    Ok(ExtraInstallCliReport {
        operation: "install-extras",
        status: if write { "written" } else { "dry-run" },
        groups: report.groups,
        entries: report.entries,
        manifest: report.manifest_path,
        staging_dir: report.staging_dir,
        target_dir: report.target_dir,
        staging_set_sha256: report.staging_set_sha256,
        target_set_sha256_before: report.target_set_sha256,
        backup_paths,
    })
}

fn rollback_extras(manifest: PathBuf) -> Result<ExtraRollbackCliReport, ConversionError> {
    let manifest_bytes = read_file(&manifest, "reading ExtData rollback manifest")?;
    let recorded: ExtraInstallManifest = serde_json::from_slice(&manifest_bytes)?;
    rollback_extra_groups(&manifest)?;
    Ok(ExtraRollbackCliReport {
        operation: "rollback-extras",
        status: "rolled-back",
        groups: recorded.groups,
        entries: recorded.entries,
        manifest,
    })
}

fn rollback_save(manifest: PathBuf) -> Result<Report, ConversionError> {
    rollback(&manifest)?;
    Ok(Report {
        profile: None,
        size: None,
        hashes: BTreeMap::new(),
        output: None,
        backup: None,
        manifest: Some(manifest),
        status: "rolled-back",
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::error::ErrorKind;
    use mh3g_save_convert::{
        events::SIMPLE_EVENT_START,
        profile::{JP_3DS_HEADER, THREE_DS_SIZE, THREE_DS_SYSTEM_SIZE},
        transforms::QUEST_COMPLETION_START,
    };

    #[test]
    fn cli_reports_the_packaged_version() {
        let error = Cli::try_parse_from(["mh3g-save-convert", "--version"]).unwrap_err();

        assert_eq!(error.kind(), ErrorKind::DisplayVersion);
        assert!(error.to_string().contains(env!("CARGO_PKG_VERSION")));
    }

    #[test]
    fn compatibility_manifest_v2_renames_output_directory_but_reads_v1_alias() {
        let legacy: CompatibilityRepairManifest = serde_json::from_str(
            r#"{
                "version": 1,
                "transaction_id": "legacy-id",
                "current_dir": "legacy-output",
                "source_set_sha256": "source",
                "current_set_sha256": "current",
                "preview_sha256": "preview",
                "core_manifest": "core.json",
                "extras_manifest": null
            }"#,
        )
        .unwrap();
        assert_eq!(legacy.output_dir, PathBuf::from("legacy-output"));
        assert_eq!(legacy.output_set_sha256, None);

        let current = CompatibilityRepairManifest {
            version: COMPATIBILITY_REPAIR_MANIFEST_VERSION,
            transaction_id: "current-id".to_owned(),
            output_dir: PathBuf::from("repaired-output"),
            source_set_sha256: "source".to_owned(),
            current_set_sha256: "current".to_owned(),
            output_set_sha256: Some("output".to_owned()),
            preview_sha256: "preview".to_owned(),
            core_manifest: Some(PathBuf::from("core.json")),
            extras_manifest: None,
        };
        let encoded = serde_json::to_value(current).unwrap();
        assert_eq!(encoded["output_dir"], "repaired-output");
        assert!(encoded.get("current_dir").is_none());
    }

    #[test]
    fn convert_extras_accepts_an_explicit_guild_card_reset_flag() {
        let parsed = Cli::try_parse_from([
            "mh3g-save-convert",
            "convert-extras",
            "--source-dir",
            "source",
            "--output-dir",
            "target",
            "--reset-guild-cards",
        ]);

        assert!(parsed.is_ok());
    }

    #[test]
    fn convert_extras_resets_nonempty_guild_cards_only_when_explicitly_requested() {
        let temp = tempfile::tempdir().unwrap();
        let source_dir = temp.path().join("3ds-extdata");
        let output_dir = temp.path().join("cemu-extras");
        fs::create_dir(&source_dir).unwrap();
        for component in EXTERNAL_COMPONENT_NAMES {
            let size = match component {
                "card1" | "card2" | "card3" => 0x58_000,
                "cardbox" => 0x30_000,
                "quest1" | "quest2" | "quest3" | "quest4" => 0x29_000,
                _ => unreachable!(),
            };
            let mut bytes = vec![0_u8; size];
            bytes[..JP_3DS_HEADER.len()].copy_from_slice(&JP_3DS_HEADER);
            if component == "card1" {
                bytes[4..8].copy_from_slice(&[0x01, 0x00, 0x00, 0x00]);
            }
            fs::write(source_dir.join(component), bytes).unwrap();
        }

        let cli = Cli::try_parse_from([
            "mh3g-save-convert",
            "convert-extras",
            "--source-dir",
            source_dir.to_str().unwrap(),
            "--output-dir",
            output_dir.to_str().unwrap(),
            "--write",
            "--reset-guild-cards",
        ])
        .unwrap();

        run(cli).unwrap();

        let card1 = fs::read(output_dir.join("card1")).unwrap();
        assert_eq!(card1.len(), 0x58_024);
        assert!(card1[40..].iter().all(|byte| *byte == 0));
    }

    #[test]
    fn convert_system_dry_run_preserves_the_existing_target_file() {
        let temp = tempfile::tempdir().unwrap();
        let source_dir = temp.path().join("source");
        let output_dir = temp.path().join("output");
        fs::create_dir(&source_dir).unwrap();
        fs::create_dir(&output_dir).unwrap();
        let source = source_dir.join("system");
        let output = output_dir.join("system");
        let mut bytes = vec![0_u8; THREE_DS_SYSTEM_SIZE];
        bytes[..JP_3DS_HEADER.len()].copy_from_slice(&JP_3DS_HEADER);
        fs::write(&source, bytes).unwrap();
        let mut current = mh3g_save_convert::profile::build_jp_cemu_header(
            "system",
            mh3g_save_convert::profile::SYSTEM_PAYLOAD_SIZE,
        )
        .unwrap()
        .to_vec();
        current.resize(mh3g_save_convert::profile::CEMU_SYSTEM_SIZE, 0);
        fs::write(&output, &current).unwrap();

        let report = convert_system(source, output.clone(), None, None, false, false).unwrap();

        assert_eq!(report.profile, Some(SaveProfile::JpCemuSystem));
        assert_eq!(report.status, "dry-run");
        assert_eq!(report.output, Some(output.clone()));
        assert_eq!(fs::read(output).unwrap(), current);
    }

    #[test]
    fn convert_extras_wraps_all_shared_components_without_writing_on_dry_run() {
        let temp = tempfile::tempdir().unwrap();
        let source_dir = temp.path().join("3ds-extdata");
        let output_dir = temp.path().join("cemu-extras");
        fs::create_dir(&source_dir).unwrap();
        for component in EXTERNAL_COMPONENT_NAMES {
            let size = match component {
                "card1" | "card2" | "card3" => 0x58_000,
                "cardbox" => 0x30_000,
                "quest1" | "quest2" | "quest3" | "quest4" => 0x29_000,
                _ => unreachable!(),
            };
            let mut bytes = vec![0_u8; size];
            bytes[..JP_3DS_HEADER.len()].copy_from_slice(&JP_3DS_HEADER);
            if component.starts_with("quest") {
                bytes[4] = component.as_bytes()[4 % component.len()];
            }
            fs::write(source_dir.join(component), bytes).unwrap();
        }

        let report = convert_extras(source_dir, output_dir.clone(), true, false, false).unwrap();

        assert_eq!(report.status, "dry-run");
        assert_eq!(report.components.len(), EXTERNAL_COMPONENT_NAMES.len());
        assert!(!output_dir.exists());
        assert!(report.components.iter().all(|component| component.size
            == match component.component.as_str() {
                "card1" | "card2" | "card3" => 0x58_024,
                "cardbox" => 0x30_024,
                "quest1" | "quest2" | "quest3" | "quest4" => 0x29_024,
                _ => unreachable!(),
            }));
    }

    #[test]
    fn inspect_progress_reports_the_source_quest_state_by_semantic_id() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("user2");
        let mut bytes = vec![0_u8; THREE_DS_SIZE];
        bytes[..JP_3DS_HEADER.len()].copy_from_slice(&JP_3DS_HEADER);
        let offset = JP_3DS_HEADER.len() + QUEST_COMPLETION_START;
        bytes[offset..offset + 4].copy_from_slice(&(1_u32 << 10).to_le_bytes());
        fs::write(&source, bytes).unwrap();

        let report = inspect_progress(source, None, Some(1204)).unwrap();

        assert_eq!(report.quests.len(), 1);
        assert_eq!(report.quests[0].quest_id, 1204);
        assert_eq!(report.quests[0].title_en.as_deref(), Some("Bear Trap"));
        assert!(report.quests[0].source_completed);
        assert_eq!(report.quests[0].target_completed, None);
    }

    #[test]
    fn inspect_events_reports_only_active_events_by_default() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("user2");
        let mut bytes = vec![0_u8; THREE_DS_SIZE];
        bytes[..JP_3DS_HEADER.len()].copy_from_slice(&JP_3DS_HEADER);
        let event_id = 468_usize;
        let offset = JP_3DS_HEADER.len() + SIMPLE_EVENT_START + event_id / 16 * 2;
        bytes[offset..offset + 2].copy_from_slice(&(1_u16 << (event_id % 16)).to_le_bytes());
        fs::write(&source, bytes).unwrap();

        let report = inspect_events(source, None, false).unwrap();

        assert_eq!(report.simple_events.len(), 1);
        assert_eq!(report.simple_events[0].event_id, 468);
        assert!(report.simple_events[0].source_set);
        assert_eq!(report.simple_events[0].target_set, None);
    }
}
