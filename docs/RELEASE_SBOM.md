# Release SBOM contract

`scripts/generate-sbom.py` produces deterministic CycloneDX 1.5 JSON from a
clean, exact Git commit. Its canonical self-verifier is the release gate; this
repository does not claim validation by an external CycloneDX schema service.

## Dependency SBOM

The existing positional entry point remains compatible:

```sh
python3 scripts/generate-sbom.py artifacts/sbom/mh-save-sync.cdx.json
```

The explicit equivalent and verifier are:

```sh
python3 scripts/generate-sbom.py dependencies artifacts/sbom/mh-save-sync.cdx.json
python3 scripts/generate-sbom.py verify-dependencies --sbom artifacts/sbom/mh-save-sync.cdx.json
```

Only crates.io registry packages with a Cargo.lock SHA-256 are listed. Workspace
and path crates are not assigned invented external checksums.

## Artifact-bound release SBOM

The identity JSON must bind `source_ref` to the clean checked-out HEAD and
contain exactly one of every local/core kind:

- `rust-cli`, `rust-server`, `android-apk`
- `macos-app`, `macos-cli`
- `mh3g-converter-cli`, `mh3g-converter-macos`

The following distribution kinds are allowlisted and optional during local
generation: `macos-save-sync-zip`, `mh3g-converter-macos-zip`,
`mh3g-converter-windows-zip`, `mh3g-converter-windows-portable`, and
`mh3g-converter-windows-setup`. If present, each must be unique, exist, be
non-empty, use the expected role, and is hashed into the BOM. Unknown kinds are
rejected. Windows distributions that cannot be built locally remain
**Unverified**; the final merged-ref release identity must add every actual
locked ZIP/portable/setup artifact before alpha.9 can claim full coverage.

```sh
python3 scripts/generate-sbom.py release \
  --identity /path/to/release-identity.json \
  --output /path/to/mh-save-sync.release.cdx.json \
  --receipt /path/to/mh-save-sync.sbom-identity.json
python3 scripts/generate-sbom.py verify-release \
  --identity /path/to/release-identity.json \
  --sbom /path/to/mh-save-sync.release.cdx.json \
  --receipt /path/to/mh-save-sync.sbom-identity.json
```

The receipt binds `format=cyclonedx-json`, the exact source ref, the SBOM
SHA-256, and the primary Android APK SHA-256. The Cargo.lock aggregate is bound
to its exact file SHA and depends on only registry packages proven by lockfile
checksums. Individual Rust artifacts do not claim an unproven per-binary
dependency closure.
