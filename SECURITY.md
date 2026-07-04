# Security Policy

## Reporting

Use GitHub private vulnerability reporting for `MHToolkit/mh-save-sync`.
Do not file public issues containing secrets, real save data or exploitable
details.

## Supported versions

No stable version exists yet. Only the newest alpha branch receives fixes.

## Security invariants

- Recovery and device secrets remain client-side.
- Stored and transmitted save content is authenticated ciphertext.
- Device revocation blocks future server authorization but cannot erase key
  material already copied from a compromised device.
- Restore input is rejected on traversal, absolute paths, symlinks, duplicate
  paths, case collisions, quota excess or authentication failure.
- Logs and diagnostics must not contain secrets or plaintext save content.

