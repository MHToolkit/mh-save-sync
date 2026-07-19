use std::{collections::BTreeMap, fs, path::PathBuf, process};

use clap::{Parser, Subcommand};
use mh3g_save_convert::{
    ConversionError,
    converter::convert_3ds_to_cemu,
    profile::{SaveProfile, inspect_bytes, validate_slot_path},
    transaction::{install, manifest_path_for_target, rollback},
};
use serde::Serialize;

#[derive(Debug, Parser)]
#[command(name = "mh3g-save-convert")]
#[command(about = "Convert Japanese MH3G 3DS save slots to Cemu")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Inspect a Japanese MH3G save without changing it.
    Inspect { source: PathBuf },
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

fn main() {
    if let Err(error) = run(Cli::parse()) {
        eprintln!("{error}");
        process::exit(1);
    }
}

fn run(cli: Cli) -> Result<(), ConversionError> {
    let report = match cli.command {
        Command::Inspect { source } => inspect(source)?,
        Command::Convert {
            source,
            output,
            dry_run,
            write,
        } => convert(source, output, dry_run, write)?,
        Command::Rollback { manifest } => rollback_save(manifest)?,
    };
    println!("{}", serde_json::to_string(&report)?);
    Ok(())
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
    debug_assert!(!(dry_run && write));
    validate_slot_path(&source)?;
    validate_slot_path(&output)?;
    if source.file_name() != output.file_name() {
        return Err(ConversionError::InvalidSave(format!(
            "source and output save slot names must match: {} != {}",
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
    let converted = convert_3ds_to_cemu(&source_bytes)?;
    let converted_inspection = inspect_bytes(&converted)?;
    debug_assert_eq!(converted_inspection.profile, SaveProfile::JpCemu);

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
