---
id: VOL-006
title: "SFTP network backend over OpenDAL"
status: To Do
assignee: anhvy
parent: VOL-000
priority: High
tags: [volume, network, sftp, opendal, backend]
whitepaper: docs/core/design/network-storage.md
last_updated: 2026-10-05
related_tasks: [VOL-004, CLOUD-003]
dependencies: []
---

## Description

Add the storage-backend half of network volumes: a `NetworkBackend` that
speaks SFTP through OpenDAL 0.54, behind the existing `VolumeBackend` trait
so the indexer needs no changes. Mirrors `CloudBackend` in
`core/src/volume/backend/cloud.rs`. Key auth only (OpenDAL SFTP has no
password login by upstream design); the key path is passed through to
`openssh` `SessionBuilder::keyfile`, omitted entirely when the user wants
plain-`ssh` default identity/agent behavior.

## Implementation Steps

- [ ] Enable `services-sftp` in the `opendal` dependency in `core/Cargo.toml`
- [ ] Add `NetworkProtocol` enum (`Sftp`, `Webdav` reserved) in
  `core/src/volume/backend/mod.rs`
- [ ] Add `BackendType::Network(NetworkProtocol)` alongside
  `BackendType::Local` / `Cloud`
- [ ] Create `core/src/volume/backend/network.rs` with `NetworkBackend`:
  `new_sftp { host, port, username, key_path: Option<PathBuf>, root }`,
  full `VolumeBackend` trait impl (`read`, `read_range`, `read_dir`,
  `metadata`, `exists`, `delete`, `create_directory`), `is_local() == false`
- [ ] Fail closed with `VolumeError::Platform` messages that name the
  failing stage (DNS/connect, auth, known-hosts) — the action surfaces
  these as validation errors
- [ ] Default `known_hosts_strategy` Strict; document why in code

## Acceptance Criteria

- [ ] `cargo build` passes with the new feature enabled
- [ ] Unit tests: builder rejects empty host / bad port; trait methods
  exercised against a local `sftp-server` via OpenSSH client on dev
  machines (documented, skipped when absent)
- [ ] `cargo clippy` and `cargo fmt` clean for touched files
