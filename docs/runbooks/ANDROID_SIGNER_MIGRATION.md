# Android signer migration runbook

## Purpose and safety boundary

The installed `org.mhtoolkit.savesync` Alpha build is signed by the local
Android debug key while production builds use the dedicated MH Save Sync
release key. Uninstalling the app would remove app-private onboarding, wrapped
key and queue state. This runbook creates an Android APK Signature Scheme v3
proof-of-rotation so Android can accept the production signer as an update while
preserving installed data.

This procedure is offline-only until a user explicitly authorizes device
installation. It does not invoke ADB, uninstall, clear data, inspect save trees,
or touch Nemessix.

## Confirmed starting state

The archived APK that was installed during the Alpha validation is
`mh-save-sync-63da8e5-debug.apk`, SHA-256
`8e32a75bd41e0de8c28db0640d058822dda2e57bc851a795d1c2da7689958b9f`.
Read-only `apksigner verify --verbose --print-certs` confirms:

- package `org.mhtoolkit.savesync`, versionCode `3`, versionName
  `0.1.0-alpha.3`;
- exactly one current signer, certificate SHA-256
  `ef44f7a19b5029bda21cb2644b8d3ec49d17633d49e0e165b42f991cfe5adedb`;
- v1 `false`, v2 `true`, v3 `false`, v3.1 `false`, v4 `false`.

The fingerprint is an exact match for the predecessor key that has now been
sealed at `~/Documents/Secrets/mh-save-sync-android-old-signer.keystore` with
mode `0600`, so the private key required to authorize the rotation remains
locally controllable without depending on mutable Android SDK state. The
production certificate SHA-256 is
`faa3b4e94c753bb385b3f2961de7191e5ca9f7e124f0e4a45526b3524efd28f3`.

## Platform decision

Android 9 introduced APK key rotation through APK Signature Scheme v3. The new
APK embeds a proof-of-rotation linked list in which each prior key signs its
successor. AOSP explicitly says rotation is not recommended on Android 12
(API 31) and earlier. Android 13 (API 33) and newer make `checkSignatures`
recognize the newest certificate. Android 16 is therefore inside the supported
and recommended platform range, while this migration's authorized device gate
is deliberately limited to API 33 or newer.

Official sources, accessed 2026-07-18:

- <https://source.android.com/docs/security/features/apksigning/v3>
- <https://developer.android.com/about/versions/pie/android-9.0#apk-key-rotation>
- Android SDK Build Tools 36.0.0 `apksigner rotate`, `sign`, and `lineage`
  command help installed locally.

The lineage grants the predecessor only `installed-data`. It deliberately does
not grant shared UID, signature permission, rollback, or authenticator
capabilities. In particular, rollback remains false so the debug key cannot
retake control after migration.

## Secret layout

The binary lineage is stored at:

```text
~/Documents/Secrets/mh-save-sync-android-signing-lineage.bin
```

The predecessor password environment is stored separately at
`~/Documents/Secrets/mh-save-sync-android-old-signer.env`, also mode `0600`.
The sealed predecessor keystore is
`~/Documents/Secrets/mh-save-sync-android-old-signer.keystore`, mode `0600`.

It must remain mode `0600`. The old and production keystores, passwords, and the
lineage binary never enter Git, build logs, evidence JSON, or release archives.
Only certificate and artifact hashes are public evidence.

## Reproducible offline packaging

The packaging script refuses dirty Git state, missing secrets, wrong file
modes, an unexpected predecessor certificate, an unexpected production
certificate, a lineage without installed-data capability, a lineage that lets
the debug signer roll back, or a versionCode not greater than installed
versionCode `3`.

```bash
rtk scripts/android-package-signer-migration.sh
```

Default migration identity:

- versionCode `4`;
- versionName `0.1.0-alpha.3-signer-migration.1`;
- rotation minimum SDK `28`;
- current signer: production certificate;
- predecessor: installed debug certificate with installed-data capability.

Pure offline verification:

```bash
rtk env JAVA_HOME="/Applications/Android Studio.app/Contents/jbr/Contents/Home" \
  "$HOME/Library/Android/sdk/build-tools/36.0.0/apksigner" \
  verify --verbose --print-certs <migration.apk>

rtk env JAVA_HOME="/Applications/Android Studio.app/Contents/jbr/Contents/Home" \
  "$HOME/Library/Android/sdk/build-tools/36.0.0/apksigner" \
  lineage --in <migration.apk> --print-certs
```

Expected result: v3 verification succeeds, the current signer is the
production certificate, lineage signer #1 is the installed debug certificate,
lineage signer #2 is the production certificate, and installed-data capability
is true.

The verified offline artifact built from packaging commit `3d49c7d` is
`mh-save-sync-3d49c7d-signer-migration.apk`, SHA-256
`b587761d4d3ef8a1e3ff3a1b7a67ce5f893d11283d86f0ad826bb15986246a92`.
It has versionCode `4`, v3 `true`, and production as its current signer. Its v2
status is `false` because the app minimum SDK is 29 and rotation minimum SDK is
28, so every supported platform verifies the v3 production signature and
lineage rather than a legacy v2 signature.

## Authorized device migration gate

No device action belongs to this PR. A later, explicitly authorized acceptance
is permitted only when the target reports API level 33 or newer. It
must first export the recovery phrase and a redacted application-state inventory,
then record hashes of the app-private database and preferences through a
platform-supported backup path if available. Only after those checkpoints may
the migration APK be installed as an in-place update. Acceptance requires the
package data directory to remain intact, onboarding and wrapped account secret
to remain readable, queued work to remain present, and PackageManager to report
the production certificate as the current signer with the debug certificate in
history.

If an actual device rejects the v3 lineage despite the offline gates, stop. Do
not uninstall or clear data. The fallback is a separately authorized export,
uninstall/reinstall, recovery-phrase import and itemized hash reconciliation;
it is not automatic and must never be attempted without the user's approval.
