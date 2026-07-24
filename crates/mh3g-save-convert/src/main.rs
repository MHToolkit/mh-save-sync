use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process,
};

use clap::{Parser, Subcommand};
use mh3g_save_convert::{
    ConversionError,
    converter::{
        EXTERNAL_COMPONENT_NAMES, convert_external_component_to_cemu_named, convert_source_to_cemu,
    },
    events::event_snapshot,
    profile::{SaveProfile, inspect_bytes, validate_slot_path, validate_system_path},
    progress::quest_progress,
    transaction::{install, manifest_path_for_target, rollback, sha256_hex},
};
use serde::Serialize;

#[derive(Debug, Parser)]
#[command(name = "mh3g-save-convert")]
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
    /// Convert a Japanese MH3G 3DS slot, dry-running unless --write is given.
    Convert {
        source: PathBuf,
        #[arg(long)]
        output: PathBuf,
        #[arg(long, conflicts_with = "write")]
        dry_run: bool,
        #[arg(long, conflicts_with = "dry_run")]
        write: bool,
    },
    /// Convert the Japanese MH3G 3DS shared system data, dry-running unless --write is given.
    ConvertSystem {
        source: PathBuf,
        #[arg(long)]
        output: PathBuf,
        #[arg(long, conflicts_with = "write")]
        dry_run: bool,
        #[arg(long, conflicts_with = "dry_run")]
        write: bool,
    },
    /// Package MH3G 3DS extdata (guild cards and quests) for Cemu without overwriting a save.
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
        Command::ConvertExtras {
            source_dir,
            output_dir,
            dry_run,
            write,
        } => println!(
            "{}",
            serde_json::to_string(&convert_extras(source_dir, output_dir, dry_run, write)?)?
        ),
        command => {
            let report = match command {
                Command::Inspect { source } => inspect(source)?,
                Command::Convert {
                    source,
                    output,
                    dry_run,
                    write,
                } => convert(source, output, dry_run, write)?,
                Command::ConvertSystem {
                    source,
                    output,
                    dry_run,
                    write,
                } => convert_system(source, output, dry_run, write)?,
                Command::Rollback { manifest } => rollback_save(manifest)?,
                Command::InspectProgress { .. }
                | Command::InspectEvents { .. }
                | Command::ConvertExtras { .. } => unreachable!(),
            };
            println!("{}", serde_json::to_string(&report)?);
        }
    }
    Ok(())
}

fn inspect_progress(
    source: PathBuf,
    target: Option<PathBuf>,
    quest_id: Option<u16>,
) -> Result<ProgressReport, ConversionError> {
    validate_slot_path(&source)?;
    let source_bytes = fs::read(&source)?;
    let source_inspection = inspect_bytes(&source_bytes)?;
    let source_progress = quest_progress(&source_bytes)?;

    let (target_inspection, target_progress) = if let Some(path) = target.as_ref() {
        validate_slot_path(path)?;
        let bytes = fs::read(path)?;
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
    let source_bytes = fs::read(&source)?;
    let source_inspection = inspect_bytes(&source_bytes)?;
    let compare_all = all || target.is_some();
    let source_events = event_snapshot(&source_bytes, compare_all)?;

    let (target_inspection, target_events) = if let Some(path) = target.as_ref() {
        validate_slot_path(path)?;
        let bytes = fs::read(path)?;
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
    let source_bytes = fs::read(source)?;
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
    dry_run: bool,
    write: bool,
) -> Result<Report, ConversionError> {
    convert_component(
        source,
        output,
        dry_run,
        write,
        validate_slot_path,
        SaveProfile::JpThreeDs,
        SaveProfile::JpCemu,
    )
}

fn convert_system(
    source: PathBuf,
    output: PathBuf,
    dry_run: bool,
    write: bool,
) -> Result<Report, ConversionError> {
    convert_component(
        source,
        output,
        dry_run,
        write,
        validate_system_path,
        SaveProfile::JpThreeDsSystem,
        SaveProfile::JpCemuSystem,
    )
}

fn convert_component(
    source: PathBuf,
    output: PathBuf,
    dry_run: bool,
    write: bool,
    validate_path: fn(&Path) -> Result<(), ConversionError>,
    expected_source_profile: SaveProfile,
    expected_output_profile: SaveProfile,
) -> Result<Report, ConversionError> {
    debug_assert!(!(dry_run && write));
    validate_path(&source)?;
    validate_path(&output)?;
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

    let source_bytes = fs::read(source)?;
    let source_inspection = inspect_bytes(&source_bytes)?;
    if source_inspection.profile != expected_source_profile {
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
    debug_assert_eq!(converted_inspection.profile, expected_output_profile);

    let mut report = Report {
        profile: Some(converted_inspection.profile),
        size: Some(converted_inspection.size),
        hashes: BTreeMap::from([
            ("source".to_owned(), source_inspection.sha256),
            ("output".to_owned(), converted_inspection.sha256),
        ]),
        output: Some(output.clone()),
        backup: None,
        manifest: None,
        status: "dry-run",
    };

    if write {
        let manifest_path = manifest_path_for_target(&output)?;
        let manifest = install(&source_bytes, &converted, &output, &manifest_path)?;
        report.backup = manifest.backup;
        report.manifest = Some(manifest_path);
        report.status = "written";
    } else {
        debug_assert!(dry_run || !write);
    }

    Ok(report)
}

fn convert_extras(
    source_dir: PathBuf,
    output_dir: PathBuf,
    dry_run: bool,
    write: bool,
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
        let source_bytes = fs::read(&source)?;
        let output_bytes = convert_external_component_to_cemu_named(&source_bytes, component)?;
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
        fs::create_dir_all(&output_dir)?;
        for ((_, _, output_bytes), output) in converted.iter().zip(output_paths.iter()) {
            fs::write(output, output_bytes)?;
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
    use mh3g_save_convert::{
        events::SIMPLE_EVENT_START,
        profile::{JP_3DS_HEADER, THREE_DS_SIZE, THREE_DS_SYSTEM_SIZE},
        transforms::QUEST_COMPLETION_START,
    };

    #[test]
    fn convert_system_dry_run_never_creates_the_target_file() {
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

        let report = convert_system(source, output.clone(), false, false).unwrap();

        assert_eq!(report.profile, Some(SaveProfile::JpCemuSystem));
        assert_eq!(report.status, "dry-run");
        assert_eq!(report.output, Some(output.clone()));
        assert!(!output.exists());
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
            bytes[4] = component.as_bytes()[4 % component.len()];
            fs::write(source_dir.join(component), bytes).unwrap();
        }

        let report = convert_extras(source_dir, output_dir.clone(), true, false).unwrap();

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
