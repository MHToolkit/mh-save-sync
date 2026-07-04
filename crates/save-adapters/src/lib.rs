use save_domain::{
    AdapterCapabilities, AdapterDescriptor, Platform, RestorePolicy, RootAcquisition,
    StabilityPolicy, SupportLevel,
};

pub fn all_descriptors() -> Vec<AdapterDescriptor> {
    vec![
        nemessix_macos(),
        nemessix_android(),
        azahar_android(),
        citra_mmj_android(),
        generic_folder_macos(),
        generic_folder_android(),
        citra_classic_macos_experimental(),
        azahar_macos_experimental(),
        ppsspp_contract(),
        dolphin_contract(),
        pcsx2_nethersx2_contract(),
        switch_family_contract(),
    ]
}

fn three_ds_contract() -> String {
    "sdmc/Nintendo 3DS/<system_id>/<sd_id>/title/<title_high>/<title_low>/data/00000001; extdata optional but separate; no region conversion".into()
}

fn three_ds_excludes() -> Vec<String> {
    vec![
        "cache".into(),
        "shaders".into(),
        "load/textures".into(),
        "cheats".into(),
        "screenshots".into(),
        "dump".into(),
        "log".into(),
        "config".into(),
        "states".into(),
    ]
}

fn standard_caps(save_complete_event: bool, saf: bool) -> AdapterCapabilities {
    AdapterCapabilities {
        save_complete_event,
        launch_gate: true,
        exit_reconcile: true,
        dirty_observer: true,
        saf_restore_journal: saf,
    }
}

fn standard_restore(atomic_replace: bool, saf_journal: bool) -> RestorePolicy {
    RestorePolicy {
        require_emulator_stopped: true,
        require_pre_restore_snapshot: true,
        atomic_replace,
        saf_journal,
    }
}

pub fn nemessix_macos() -> AdapterDescriptor {
    AdapterDescriptor {
        emulator_id: "nemessix-3ds".into(),
        platform: Platform::Macos,
        bundle_ids: vec!["io.github.vincentadamnemessisx.nemessix".into()],
        package_ids: vec![],
        process_names: vec!["Nemessix".into(), "nemessix".into()],
        root_acquisition: RootAcquisition::NativePath,
        user_root_hint: Some("~/Library/Application Support/Nemessix".into()),
        game_key_contract: three_ds_contract(),
        include_globs: vec!["sdmc/Nintendo 3DS/*/*/title/*/*/data/00000001/**".into()],
        exclude_globs: three_ds_excludes(),
        capabilities: standard_caps(false, false),
        stability: StabilityPolicy::default(),
        restore: standard_restore(true, false),
        support_level: SupportLevel::PathVerified,
        evidence_fingerprint: "2026-07-04 bundle=c836bc1 app-md5=7e933c163ed6a9b55fd284ef26eaff08 titles=0004000000048100:3files53764".into(),
    }
}

pub fn nemessix_android() -> AdapterDescriptor {
    AdapterDescriptor {
        emulator_id: "nemessix-3ds".into(),
        platform: Platform::Android,
        bundle_ids: vec![],
        package_ids: vec!["io.github.vincentadamnemessisx.nemessix".into()],
        process_names: vec!["io.github.vincentadamnemessisx.nemessix".into()],
        root_acquisition: RootAcquisition::SafTree,
        user_root_hint: Some("SAF tree selected by user; observed ADB root /storage/emulated/0/Games/Nemessix".into()),
        game_key_contract: three_ds_contract(),
        include_globs: vec!["sdmc/Nintendo 3DS/*/*/title/*/*/data/00000001/**".into()],
        exclude_globs: three_ds_excludes(),
        capabilities: standard_caps(false, true),
        stability: StabilityPolicy::default(),
        restore: standard_restore(false, true),
        support_level: SupportLevel::PathVerified,
        evidence_fingerprint: "2026-07-04 package=f0767428c-vanilla title=0004000000048100 files=2 bytes=47616 fp=dd93905a".into(),
    }
}

pub fn azahar_android() -> AdapterDescriptor {
    AdapterDescriptor {
        emulator_id: "azahar-3ds".into(),
        platform: Platform::Android,
        bundle_ids: vec![],
        package_ids: vec!["org.azahar_emu.azahar".into()],
        process_names: vec!["org.azahar_emu.azahar".into()],
        root_acquisition: RootAcquisition::SafTree,
        user_root_hint: None,
        game_key_contract: three_ds_contract(),
        include_globs: vec!["sdmc/Nintendo 3DS/*/*/title/*/*/data/00000001/**".into()],
        exclude_globs: three_ds_excludes(),
        capabilities: standard_caps(false, true),
        stability: StabilityPolicy::default(),
        restore: standard_restore(false, true),
        support_level: SupportLevel::Experimental,
        evidence_fingerprint:
            "2026-07-04 package=2126.0-alpha2-vanilla installed; no save root observed".into(),
    }
}

pub fn citra_mmj_android() -> AdapterDescriptor {
    AdapterDescriptor {
        emulator_id: "citra-mmj-3ds".into(),
        platform: Platform::Android,
        bundle_ids: vec![],
        package_ids: vec!["org.citra.emu".into()],
        process_names: vec!["org.citra.emu".into()],
        root_acquisition: RootAcquisition::SafTree,
        user_root_hint: Some("SAF tree selected by user; observed ADB root /storage/emulated/0/citra-emu".into()),
        game_key_contract: three_ds_contract(),
        include_globs: vec!["sdmc/Nintendo 3DS/*/*/title/*/*/data/00000001/**".into()],
        exclude_globs: three_ds_excludes(),
        capabilities: standard_caps(false, true),
        stability: StabilityPolicy::default(),
        restore: standard_restore(false, true),
        support_level: SupportLevel::PathVerified,
        evidence_fingerprint: "2026-07-04 package=20220729-mh-rpc.2 title=0004000000048100 files=2 bytes=47616 fp=dd93905a".into(),
    }
}

pub fn generic_folder_macos() -> AdapterDescriptor {
    generic_folder(Platform::Macos, RootAcquisition::UserSelectedFolder, true)
}

pub fn generic_folder_android() -> AdapterDescriptor {
    generic_folder(Platform::Android, RootAcquisition::SafTree, false)
}

fn generic_folder(
    platform: Platform,
    root_acquisition: RootAcquisition,
    atomic: bool,
) -> AdapterDescriptor {
    AdapterDescriptor {
        emulator_id: "generic-folder".into(),
        platform,
        bundle_ids: vec![],
        package_ids: vec![],
        process_names: vec![],
        root_acquisition,
        user_root_hint: None,
        game_key_contract:
            "user-selected folder; title/region/slot supplied by profile; no hidden conversion"
                .into(),
        include_globs: vec!["**".into()],
        exclude_globs: vec![
            "cache".into(),
            "shaders".into(),
            "load/textures".into(),
            "cheats".into(),
            "screenshots".into(),
            "config".into(),
        ],
        capabilities: AdapterCapabilities {
            save_complete_event: false,
            launch_gate: false,
            exit_reconcile: true,
            dirty_observer: true,
            saf_restore_journal: !atomic,
        },
        stability: StabilityPolicy::default(),
        restore: standard_restore(atomic, !atomic),
        support_level: SupportLevel::FixtureVerified,
        evidence_fingerprint: "synthetic fixture tests".into(),
    }
}

pub fn citra_classic_macos_experimental() -> AdapterDescriptor {
    let mut d = nemessix_macos();
    d.emulator_id = "citra-classic-3ds".into();
    d.bundle_ids = vec!["org.citra-emu.citra".into()];
    d.process_names = vec!["citra-qt".into(), "Citra".into()];
    d.user_root_hint = Some("~/Library/Application Support/Citra".into());
    d.support_level = SupportLevel::Experimental;
    d.evidence_fingerprint = "descriptor only; no current runtime evidence".into();
    d
}

pub fn azahar_macos_experimental() -> AdapterDescriptor {
    let mut d = nemessix_macos();
    d.emulator_id = "azahar-3ds".into();
    d.bundle_ids = vec!["org.azahar_emu.azahar".into()];
    d.process_names = vec!["Azahar".into(), "azahar".into()];
    d.user_root_hint = Some("~/Library/Application Support/Azahar".into());
    d.support_level = SupportLevel::Experimental;
    d.evidence_fingerprint = "descriptor only; no current runtime evidence".into();
    d
}

pub fn ppsspp_contract() -> AdapterDescriptor {
    contract(
        "ppsspp",
        "PSP/SAVEDATA/<game-slot>; official source/docs verification required",
    )
}

pub fn dolphin_contract() -> AdapterDescriptor {
    contract(
        "dolphin",
        "GC/Wii per-title save roots; official source/docs verification required",
    )
}

pub fn pcsx2_nethersx2_contract() -> AdapterDescriptor {
    contract(
        "pcsx2-nethersx2",
        "memory-card file as logical save unless per-title extraction is validated",
    )
}

pub fn switch_family_contract() -> AdapterDescriptor {
    contract(
        "switch-family",
        "per-title save container; no keys/firmware/ROM assumptions; legal user data only",
    )
}

fn contract(id: &str, contract: &str) -> AdapterDescriptor {
    AdapterDescriptor {
        emulator_id: id.into(),
        platform: Platform::Generic,
        bundle_ids: vec![],
        package_ids: vec![],
        process_names: vec![],
        root_acquisition: RootAcquisition::UserSelectedFolder,
        user_root_hint: None,
        game_key_contract: contract.into(),
        include_globs: vec![],
        exclude_globs: vec![],
        capabilities: AdapterCapabilities {
            save_complete_event: false,
            launch_gate: false,
            exit_reconcile: true,
            dirty_observer: true,
            saf_restore_journal: false,
        },
        stability: StabilityPolicy::default(),
        restore: standard_restore(true, false),
        support_level: SupportLevel::Experimental,
        evidence_fingerprint: "descriptor contract only".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn android_descriptors_are_distinct_and_no_unverified_runtime_claims() {
        let descriptors = all_descriptors();
        let nem = descriptors
            .iter()
            .find(|d| d.emulator_id == "nemessix-3ds" && d.platform == Platform::Android)
            .unwrap();
        let aza = descriptors
            .iter()
            .find(|d| d.emulator_id == "azahar-3ds" && d.platform == Platform::Android)
            .unwrap();
        let citra = descriptors
            .iter()
            .find(|d| d.emulator_id == "citra-mmj-3ds")
            .unwrap();
        assert_ne!(nem.package_ids, aza.package_ids);
        assert_ne!(aza.package_ids, citra.package_ids);
        assert_ne!(aza.support_level, SupportLevel::RuntimeVerified);
    }

    #[test]
    fn excludes_textures_shaders_and_cheats_by_default() {
        for d in [nemessix_macos(), nemessix_android(), citra_mmj_android()] {
            assert!(d.exclude_globs.iter().any(|x| x.contains("shaders")));
            assert!(d.exclude_globs.iter().any(|x| x.contains("textures")));
            assert!(d.exclude_globs.iter().any(|x| x.contains("cheats")));
        }
    }
}
