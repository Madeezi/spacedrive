---
id: VOL-009
title: "Network volume hardening, polling, and UAT"
status: In Progress
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

- [x] Credential rotation: re-running `volumes.add_network` for an
  existing fingerprint validates then updates the stored credential
  instead of duplicating the volume (implemented in the action)
- [x] Docs: SFTP section in `docs/core/volumes.mdx` (key setup,
  `ssh-agent` for passphrase keys, Strict host keys, manual
  freshness model)
- [ ] Polling defaults (`scan_interval`): DEFERRED — the location
  table has no `scan_interval` column and no scheduler consumes
  `needs_scan()` yet; adding both is a follow-up, locations rescan
  manually until then
- [ ] Offline flagging (`is_available`): DEFERRED — nothing reads the
  flag today; manual rescan surfaces connection errors instead
- [ ] Media-heavy sanity + full UAT against the real Jellyfin host
  over SFTP: owner's run (needs their key + live library)
- [x] UAT defect 2026-10-06: volume monitor evicted the SFTP volume
  ~1s after registration (not OS-detected), failing locations.add;
  fixed by retaining tracked volumes + surfacing backend messages
  (commit on this branch, daemon restarted, volume reload verified)

## Acceptance Criteria

- [ ] All boxes above checked against a real server, notes recorded in
  this file
- [ ] `cargo test --workspace` green; task-validator `validate` clean
- [ ] Ticket statuses updated honestly per `.tasks/Claude.md` rules
