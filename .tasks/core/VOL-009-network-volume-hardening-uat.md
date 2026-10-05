---
id: VOL-009
title: "Network volume hardening, polling, and UAT"
status: To Do
assignee: anhvy
parent: VOL-000
priority: High
tags: [volume, network, sftp, hardening, uat]
whitepaper: docs/core/design/network-storage.md
last_updated: 2026-10-05
related_tasks: [VOL-006, VOL-007, VOL-008, UI-002]
dependencies: [UI-002]
---

## Description

Close out the SFTP MVP: the volume must behave sanely when the server
goes away, credentials change, or the link is slow, and the polling
story must be deliberate rather than accidental. Last ticket in the
MVP chain.

## Implementation Steps

- [ ] Set conservative `scan_interval` default for network locations
  (1h proposed) and surface last-scan time on the Overview volume row
- [ ] Offline path: failed connection marks `is_available=false` with
  the stage-named error; daemon retries on next interval, no hot loop
- [ ] Credential rotation: re-run of `volumes.add_network` against an
  existing fingerprint updates the stored credential instead of
  duplicating the volume
- [ ] Media-heavy sanity: index a photo/music-sized library over SFTP,
  confirm ranged reads keep hashing traffic bounded; note numbers here
- [ ] Docs: short section in `docs/` covering key setup (`ssh-copy-id`,
  `ssh-agent` for passphrase keys), Strict host-key default, and the
  polling-only freshness model
- [ ] Full UAT against the real Jellyfin host over SFTP: add, browse,
  rescan, stop server (offline UI), rotate key

## Acceptance Criteria

- [ ] All boxes above checked against a real server, notes recorded in
  this file
- [ ] `cargo test --workspace` green; task-validator `validate` clean
- [ ] Ticket statuses updated honestly per `.tasks/Claude.md` rules
