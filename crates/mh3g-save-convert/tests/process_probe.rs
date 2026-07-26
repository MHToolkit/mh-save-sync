use mh3g_save_convert::{
    ConversionError,
    process_probe::{PlatformProcessProbe, ProcessEnumerator, ProcessProbe},
};

struct StaticEnumerator {
    result: Result<Vec<String>, String>,
}

impl StaticEnumerator {
    fn names(names: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            result: Ok(names.into_iter().map(Into::into).collect()),
        }
    }

    fn failure(message: impl Into<String>) -> Self {
        Self {
            result: Err(message.into()),
        }
    }
}

impl ProcessEnumerator for StaticEnumerator {
    fn process_names(&self) -> Result<Vec<String>, ConversionError> {
        self.result.clone().map_err(ConversionError::UnsafeInstall)
    }
}

#[test]
fn detects_supported_windows_emulator_names_case_insensitively() {
    let probe =
        PlatformProcessProbe::with_enumerator(StaticEnumerator::names(["Cemu_release.EXE"]));

    assert_eq!(
        probe.matching_process().unwrap().as_deref(),
        Some("Cemu_release.EXE")
    );
}

#[test]
fn detects_every_supported_emulator_frontend() {
    for name in ["Cemu.exe", "Nemessix.exe", "Azahar.exe"] {
        let probe = PlatformProcessProbe::with_enumerator(StaticEnumerator::names([name]));

        assert_eq!(probe.matching_process().unwrap().as_deref(), Some(name));
    }
}

#[test]
fn ignores_unrelated_processes() {
    let probe = PlatformProcessProbe::with_enumerator(StaticEnumerator::names([
        "Finder",
        "CemuHelper.exe",
        "AzaharHelper",
    ]));

    assert_eq!(probe.matching_process().unwrap(), None);
}

#[test]
fn enumeration_failure_is_not_treated_as_no_running_emulator() {
    let probe = PlatformProcessProbe::with_enumerator(StaticEnumerator::failure("snapshot failed"));

    assert!(matches!(
        probe.matching_process(),
        Err(ConversionError::UnsafeInstall(message)) if message == "snapshot failed"
    ));
}
