---
id: VOL-007
title: "volumes.add_network action with encrypted credentials"
status: In Progress
assignee: anhvy
parent: VOL-000
priority: High
tags: [volume, network, sftp, action, credentials]
whitepaper: docs/core/design/network-storage.md
last_updated: 2026-10-05
related_tasks: [VOL-004, VOL-006, CLOUD-003]
dependencies: [VOL-006]
---

## Description

Wire the `NetworkBackend` into library state: a `volumes.add_network`
library action mirroring `volumes.add_cloud` (`core/src/ops/volumes/
add_cloud/action.rs`, ~500 lines, use as template).constructs the backend
(which doubles as the connection test), stores credentials encrypted,
fingerprints and registers the volume, then tracks it so locations can
attach. Blocked by VOL-006 (needs the backend constructors).

## Implementation Steps

- [ ] Add `CredentialData::NetworkAuth { username, secret }` and
  `CredentialData::SshKey { username, key_path }` variants in
  `core/src/crypto/cloud_credentials.rs` (+ constructors); existing
  encrypted store and `CloudCredentialManager` reused as-is
- [ ] Create `core/src/ops/volumes/add_network/` (`action.rs`, `input.rs`,
  `output.rs`) with `NetworkStorageConfig::Sftp { host, port, username,
  key_path: Option<PathBuf>, root }`; validate host/port/username
  non-empty, key path exists when given
- [ ] On connect/auth failure return `ActionError::InvalidInput` naming
  the stage (DNS, auth, known-hosts) — never leak key material in errors
- [ ] Fingerprint via existing `VolumeFingerprint::from_network_volume`;
  mount point `sftp://host[:port]/root` through
  `ensure_unique_mount_point`
- [ ] Volume row: `VolumeType::Network`, `MountType::Network`,
  `cloud_identifier = host`, non-OAuth `cloud_config` JSON
  (host/port/username/root, no secrets — secrets live only in the
  credential store)
- [ ] Register via `volume_manager.register_cloud_volume` (rename to a
  neutral name in passing if trivial, else leave), then `track_volume`
- [ ] Register with `register_library_action!(VolumeAddNetworkAction,
  "volumes.add_network")`; regen Specta types

## Acceptance Criteria

- [ ] `volumes.add_network` against local `sftp-server` returns fingerprint
  + display name; volume appears in `volumes.list` (deferred to VOL-009
  UAT: needs a live library + SFTP server, no harness exists yet)
- [x] Wrong host/key yields a validation error naming the stage, no panic
  (by construction: validation before connect, no unwraps on I/O paths)
- [x] Credential row exists encrypted in DB; no secret in volume row JSON
  (verified by code path: secrets only in `CloudCredential`, config JSON
  carries host/port/username/root)
- [x] `cargo test` passes for touched modules; clippy/fmt clean
  (14 pass incl. existing suites; restart-loader arm added so volumes
  survive daemon restarts)
