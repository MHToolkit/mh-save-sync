use std::path::Path;

#[cfg(any(target_os = "macos", target_os = "linux"))]
use std::process::{Command, Stdio};

use crate::{ConversionError, io_at_path};

pub const GUARDED_PROCESS_NAMES: [&str; 8] = [
    "Nemessix",
    "nemessix",
    "Azahar",
    "azahar",
    "Cemu",
    "cemu",
    "Cemu_release",
    "cemu_release",
];

pub trait ProcessProbe {
    fn matching_process(&self) -> Result<Option<String>, ConversionError>;
}

pub trait ProcessEnumerator: Send + Sync {
    fn process_names(&self) -> Result<Vec<String>, ConversionError>;
}

pub struct PlatformProcessProbe {
    enumerator: Box<dyn ProcessEnumerator>,
}

impl PlatformProcessProbe {
    pub fn with_enumerator(enumerator: impl ProcessEnumerator + 'static) -> Self {
        Self {
            enumerator: Box::new(enumerator),
        }
    }
}

impl Default for PlatformProcessProbe {
    fn default() -> Self {
        Self::with_enumerator(NativeProcessEnumerator)
    }
}

impl ProcessProbe for PlatformProcessProbe {
    fn matching_process(&self) -> Result<Option<String>, ConversionError> {
        Ok(self
            .enumerator
            .process_names()?
            .into_iter()
            .find(|name| is_guarded_process_name(name)))
    }
}

struct NativeProcessEnumerator;

impl ProcessEnumerator for NativeProcessEnumerator {
    fn process_names(&self) -> Result<Vec<String>, ConversionError> {
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        {
            enumerate_with_pgrep()
        }

        #[cfg(target_os = "windows")]
        {
            enumerate_with_toolhelp()
        }

        #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
        {
            Err(ConversionError::UnsafeInstall(
                "cannot establish emulator process state on this platform".to_owned(),
            ))
        }
    }
}

fn is_guarded_process_name(name: &str) -> bool {
    let stem = name
        .strip_suffix(".exe")
        .or_else(|| {
            name.get(name.len().saturating_sub(4)..)
                .filter(|suffix| suffix.eq_ignore_ascii_case(".exe"))
                .map(|_| &name[..name.len() - 4])
        })
        .unwrap_or(name);

    GUARDED_PROCESS_NAMES
        .iter()
        .any(|guarded| stem.eq_ignore_ascii_case(guarded))
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn enumerate_with_pgrep() -> Result<Vec<String>, ConversionError> {
    let mut matches = Vec::new();
    for name in GUARDED_PROCESS_NAMES {
        let status = io_at_path(
            Command::new("pgrep")
                .args(["-x", name])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status(),
            "running emulator process probe",
            Path::new("pgrep"),
        )?;
        match status.code() {
            Some(0) => matches.push(name.to_owned()),
            Some(1) => {}
            Some(code) => {
                return Err(ConversionError::IoAtPath {
                    operation: "running emulator process probe",
                    path: Path::new("pgrep").to_path_buf(),
                    source: std::io::Error::other(format!(
                        "pgrep -x {name} exited with status {code}"
                    )),
                });
            }
            None => {
                return Err(ConversionError::IoAtPath {
                    operation: "running emulator process probe",
                    path: Path::new("pgrep").to_path_buf(),
                    source: std::io::Error::other(format!("pgrep -x {name} terminated by signal")),
                });
            }
        }
    }
    Ok(matches)
}

#[cfg(target_os = "windows")]
fn enumerate_with_toolhelp() -> Result<Vec<String>, ConversionError> {
    use std::mem::{size_of, zeroed};

    use windows_sys::Win32::{
        Foundation::{CloseHandle, INVALID_HANDLE_VALUE},
        System::Diagnostics::ToolHelp::{
            CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
            TH32CS_SNAPPROCESS,
        },
    };

    const ERROR_NO_MORE_FILES: i32 = 18;

    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if snapshot == INVALID_HANDLE_VALUE {
        return io_at_path(
            Err(std::io::Error::last_os_error()),
            "creating Windows emulator process snapshot",
            Path::new("CreateToolhelp32Snapshot"),
        );
    }

    let result = (|| {
        let mut entry: PROCESSENTRY32W = unsafe { zeroed() };
        entry.dwSize = size_of::<PROCESSENTRY32W>() as u32;
        if unsafe { Process32FirstW(snapshot, &mut entry) } == 0 {
            return io_at_path(
                Err(std::io::Error::last_os_error()),
                "reading first Windows process entry",
                Path::new("Process32FirstW"),
            );
        }

        let mut names = Vec::new();
        loop {
            let length = entry
                .szExeFile
                .iter()
                .position(|character| *character == 0)
                .unwrap_or(entry.szExeFile.len());
            names.push(String::from_utf16_lossy(&entry.szExeFile[..length]));

            if unsafe { Process32NextW(snapshot, &mut entry) } == 0 {
                let error = std::io::Error::last_os_error();
                if error.raw_os_error() == Some(ERROR_NO_MORE_FILES) {
                    break;
                }
                return io_at_path(
                    Err(error),
                    "reading next Windows process entry",
                    Path::new("Process32NextW"),
                );
            }
        }

        Ok(names)
    })();

    unsafe {
        CloseHandle(snapshot);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::is_guarded_process_name;

    #[test]
    fn process_name_matching_accepts_windows_extensions() {
        assert!(is_guarded_process_name("Cemu.exe"));
        assert!(is_guarded_process_name("Cemu_release.EXE"));
        assert!(is_guarded_process_name("Azahar.exe"));
    }

    #[test]
    fn process_name_matching_rejects_similar_processes() {
        assert!(!is_guarded_process_name("CemuHelper.exe"));
        assert!(!is_guarded_process_name("AzaharHelper"));
    }
}
