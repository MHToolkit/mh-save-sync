use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process,
};

use clap::{Parser, Subcommand};
use mh3g_save_convert::{
    ConversionError,
    converter::convert_source_to_cemu,
    events::event_snapshot,
    profile::{SaveProfile, inspect_bytes, validate_slot_path, validate_system_path},
    progress::quest_progress,
    transaction::{install, manifest_path_for_target, rollback},
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
                Command::InspectProgress { .. } | Command::InspectEvents { .. } => unreachable!(),
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
