---
id: UI-002
title: "Add Storage network step with SFTP form"
status: In Progress
assignee: anhvy
priority: High
tags: [interface, add-storage, network, sftp, modal]
whitepaper: docs/core/design/network-storage.md
last_updated: 2026-10-05
related_tasks: [VOL-007, VOL-008]
dependencies: [VOL-007, VOL-008]
---

## Description

Replace the hardcoded "Coming Soon" network branch in the Add Storage
modal with a working SFTP flow: protocol picker (existing card grid,
SFTP enabled, SMB/NFS/WebDAV still marked unimplemented) then a config
form that calls `volumes.add_network` and navigates into the created
location like the local/cloud flows do. Blocked by VOL-007 (wire
strings) and VOL-008 (TS `SdPath::Network`).

## Implementation Steps

- [ ] Enable the SFTP card in the network-provider step of
  `AddStorageModal.tsx`; keep SMB/NFS/WebDAV visibly disabled
- [ ] New `sftp-config` modal step: host, port (default 22), username,
  key path (prefill-probe `~/.ssh/id_ed25519`, `~/.ssh/id_rsa`;
  empty = SSH defaults/agent), root path; reuse `StorageDialog`,
  Back button, and CTA patterns from the cloud-config step
- [ ] On submit call `volumes.add_network`; map stage-named backend
  errors to form-level messages (DNS unreachable, auth failed,
  unknown host key) with the passphrase-agent hint for encrypted keys
- [ ] Document the `ssh-copy-id` step for users without a deployed key
  in the form helper text
- [ ] Semantic Tailwind classes only (no default-palette colors, no
  `!`-prefix important — see repo history on the v4 migration)

## Acceptance Criteria

- [ ] SFTP flow against a real server: add, browse files in Explorer,
  validation errors readable on bad host/key (live run in VOL-009 UAT)
- [x] No `tsc` errors in touched files; dialog centers, layers above
  content, closes on backdrop click (regression cover for the
  z-index/pointer-events fixes)
- [x] SMB/NFS/WebDAV cards still render disabled with accurate copy
