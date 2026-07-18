# Android release signing contract

MH Save Sync production Android releases use a dedicated signing identity. The
private key and passwords live only under `~/Documents/Secrets` and are never
stored in Git, logs, APK evidence JSON or GitHub Actions output.

## Production signer

- Certificate SHA-256:
  `faa3b4e94c753bb385b3f2961de7191e5ca9f7e124f0e4a45526b3524efd28f3`
- Subject: `CN=MH Save Sync Release, O=MHToolkit, C=CN`
- Keystore environment contract:
  `MH_SAVE_SYNC_ANDROID_KEYSTORE`,
  `MH_SAVE_SYNC_ANDROID_STORE_PASSWORD`,
  `MH_SAVE_SYNC_ANDROID_KEY_ALIAS`,
  `MH_SAVE_SYNC_ANDROID_KEY_PASSWORD`
- Local wrapper: `scripts/android-package-release.sh`

The wrapper requires a clean Git worktree before building so the commit in the
artifact filename is a truthful provenance anchor rather than a label applied
to uncommitted source.

Nemessix production builds may pin only the production certificate above for
`SaveQuiescenceV1`. The Android debug signer
`ef44f7a19b5029bda21cb2644b8d3ec49d17633d49e0e165b42f991cfe5adedb`
is internal-only and must not be treated as the production trust root.

## Read-only extraction

The public certificate fingerprint can be reproduced without printing any
keystore password:

```bash
rtk openssl x509 \
  -in ~/Documents/Secrets/mh-save-sync-android-release-cert.pem \
  -noout -fingerprint -sha256
```

The signer embedded in a built APK can be verified with:

```bash
rtk env JAVA_HOME="/Applications/Android Studio.app/Contents/jbr/Contents/Home" \
  "$HOME/Library/Android/sdk/build-tools/36.0.0/apksigner" \
  verify --print-certs <release.apk>
```

Do not install a release-signed APK over a debug-signed installation. Android
correctly rejects the signer transition; preserve data by using an explicit
migration/export plan rather than uninstalling the debug app.
