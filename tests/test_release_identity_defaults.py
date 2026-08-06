from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]


def test_android_default_identity_matches_current_release_candidate() -> None:
    gradle = (ROOT / "apps/android/app/build.gradle.kts").read_text(encoding="utf-8")

    assert '.getOrElse(5)' in gradle
    assert '.getOrElse("0.1.0-alpha.4")' in gradle


def test_macos_default_identity_matches_android() -> None:
    script = (ROOT / "scripts/build-macos-app-bundle.sh").read_text(encoding="utf-8")

    assert 'MH_SAVE_SYNC_MACOS_VERSION_NAME:-0.1.0-alpha.4' in script
    assert 'MH_SAVE_SYNC_MACOS_VERSION_CODE:-5' in script
