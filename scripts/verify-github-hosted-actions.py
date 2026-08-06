#!/usr/bin/env python3
"""Static contract for the public-repository GitHub Actions topology.

This verifier intentionally reads workflow text instead of requiring PyYAML on
all developer machines.  It guards the policy that normal CI and the MH3G
packages run on GitHub's standard hosted runners, while only a tag may publish
a release.
"""

from __future__ import annotations

from pathlib import Path
import re
import sys


ROOT = Path(__file__).resolve().parents[1]
WORKFLOWS = ROOT / ".github" / "workflows"


def read_workflow(name: str) -> str:
    path = WORKFLOWS / name
    if not path.is_file():
        raise AssertionError(f"missing workflow: {path.relative_to(ROOT)}")
    return path.read_text(encoding="utf-8")


def assert_no_self_hosted(name: str, text: str) -> None:
    if re.search(r"runs-on:\s*[^\n]*\bself-hosted\b", text, flags=re.IGNORECASE):
        raise AssertionError(f"{name} still references a self-hosted runner")


def assert_short_lived_artifacts(name: str, text: str) -> None:
    blocks = text.split("uses: actions/upload-artifact@v5")
    for index, block in enumerate(blocks[1:], start=1):
        boundary = re.search(r"\n\s*-\s+(?:name:|uses:|run:)", block)
        upload = block[: boundary.start()] if boundary else block
        retention = re.search(r"retention-days:\s*(\d+)", upload)
        if retention is None:
            raise AssertionError(
                f"{name} upload-artifact block #{index} has no retention-days"
            )
        if int(retention.group(1)) > 3:
            raise AssertionError(
                f"{name} upload-artifact block #{index} retains artifacts for "
                f"{retention.group(1)} days; maximum is 3"
            )


def main() -> int:
    ci = read_workflow("ci.yml")
    canary = read_workflow("ci-canary.yml")
    windows = read_workflow("mh3g-converter-windows.yml")
    macos = read_workflow("mh3g-converter-macos.yml")
    release = read_workflow("mh3g-converter-release.yml")
    auto_patch_release = read_workflow("mh3g-converter-auto-patch-release.yml")
    readme = (ROOT / "README.md").read_text(encoding="utf-8")
    readme_zh_cn = (ROOT / "README.zh-CN.md").read_text(encoding="utf-8")

    for name, content in {
        "ci.yml": ci,
        "ci-canary.yml": canary,
        "mh3g-converter-windows.yml": windows,
        "mh3g-converter-macos.yml": macos,
        "mh3g-converter-release.yml": release,
        "mh3g-converter-auto-patch-release.yml": auto_patch_release,
    }.items():
        assert_no_self_hosted(name, content)
        assert_short_lived_artifacts(name, content)

    if "ENABLE_GITHUB_HOSTED_JOBS" in ci:
        raise AssertionError("ci.yml still gates hosted jobs behind a repository variable")
    if "ubuntu-latest" not in ci or "macos-latest" not in ci:
        raise AssertionError("ci.yml must use standard Ubuntu and macOS hosted runners")
    if "windows-2022" not in windows:
        raise AssertionError(
            "Windows package workflow must use windows-2022 with VS 2022 Build Tools"
        )
    if "windows-2022" not in release:
        raise AssertionError(
            "release workflow must use windows-2022 with VS 2022 Build Tools"
        )
    if "package-mh3g-save-converter-windows.ps1 -Bootstrap" not in windows:
        raise AssertionError(
            "Windows package CI must bootstrap disposable hosted-runner prerequisites"
        )
    if "package-mh3g-save-converter-windows.ps1 -Bootstrap" not in release:
        raise AssertionError(
            "Windows release CI must bootstrap disposable hosted-runner prerequisites"
        )
    if "timeout-minutes: 60" not in windows:
        raise AssertionError(
            "Windows package CI must allow enough time for the one-time VS bootstrap"
        )
    if "choco install visualstudio2022-workload-vctools" not in windows:
        raise AssertionError(
            "Windows package CI must provision the Visual C++ workload without winget"
        )
    if "choco install visualstudio2022-workload-vctools" not in release:
        raise AssertionError(
            "Windows release CI must provision the Visual C++ workload without winget"
        )
    if "macos-latest" not in macos:
        raise AssertionError("macOS package workflow must use macos-latest")
    if "tags:" not in release or '"v*"' not in release:
        raise AssertionError("release workflow must be triggered by v* tags")
    if "contents: write" not in release:
        raise AssertionError("release workflow must grant contents: write only to publish")
    if "gh release create" not in release or "gh release upload" not in release:
        raise AssertionError("release workflow must create and upload a GitHub Release")
    if "GH_REPO: ${{ github.repository }}" not in release:
        raise AssertionError(
            "release workflow must set GH_REPO because its publish job has no checkout"
        )
    if "MH3GSaveConverter-Setup-x64.exe" not in release:
        raise AssertionError("release workflow must publish the Windows setup executable")
    if "MH3G-Save-Converter-macOS-arm64.zip" not in release:
        raise AssertionError("release workflow must publish the macOS arm64 archive")
    if 'MH3G_CONVERTER_UI_VERSION="${GITHUB_REF_NAME#v}"' not in release:
        raise AssertionError(
            "release workflow must derive the macOS bundle version from the v* tag"
        )
    if "pull_request:" not in auto_patch_release or "types: [closed]" not in auto_patch_release:
        raise AssertionError("auto patch workflow must run after closed pull requests")
    if "github.event.pull_request.merged == true" not in auto_patch_release:
        raise AssertionError("auto patch workflow must only process merged pull requests")
    if "github.event.pull_request.base.ref == 'main'" not in auto_patch_release:
        raise AssertionError("auto patch workflow must only process merges into main")
    if "contents: write" not in auto_patch_release or "actions: write" not in auto_patch_release:
        raise AssertionError("auto patch workflow must be allowed to push its tag and dispatch")
    if "mh3g-converter-version.py --next-patch --write" not in auto_patch_release:
        raise AssertionError("auto patch workflow must use the exact package version helper")
    if 'cargo update -p mh3g-save-convert --precise "$version"' not in auto_patch_release:
        raise AssertionError("auto patch workflow must keep Cargo.lock in sync with the bumped package")
    if "git add crates/mh3g-save-convert/Cargo.toml Cargo.lock" not in auto_patch_release:
        raise AssertionError("auto patch workflow must commit both Cargo.toml and Cargo.lock")
    if "git tag -a" not in auto_patch_release or "refs/tags/${tag}" not in auto_patch_release:
        raise AssertionError("auto patch workflow must create and push an annotated v* tag")
    if "gh workflow run mh3g-converter-release.yml --ref \"$TAG\"" not in auto_patch_release:
        raise AssertionError("auto patch workflow must explicitly dispatch the tagged release workflow")
    macos_packager = (ROOT / "scripts/build-mh3g-save-converter-macos-app.sh").read_text(encoding="utf-8")
    if 'mh3g-converter-version.py" --print' not in macos_packager:
        raise AssertionError("macOS packager must derive its default version from the package manifest")
    if "## Public GitHub Actions CI and releases" not in readme:
        raise AssertionError("README.md must document the public hosted CI policy")
    if "## 公开仓库 GitHub Actions CI 与发布" not in readme_zh_cn:
        raise AssertionError("README.zh-CN.md must document the public hosted CI policy")

    print("GitHub-hosted Actions workflow contract: ok")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except AssertionError as error:
        print(f"GitHub-hosted Actions workflow contract: failed: {error}", file=sys.stderr)
        raise SystemExit(1)
