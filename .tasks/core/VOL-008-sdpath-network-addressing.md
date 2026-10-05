---
id: VOL-008
title: "SdPath Network addressing end to end"
status: To Do
assignee: anhvy
parent: VOL-000
priority: High
tags: [volume, network, sdpath, addressing]
whitepaper: docs/core/design/network-storage.md
last_updated: 2026-10-05
related_tasks: [VOL-004, VOL-006, CORE-002]
dependencies: [VOL-006]
---

## Description

Give network volumes a first-class address: new `SdPath::Network
{ protocol: NetworkProtocol, host: String, path: String }` variant
(Rust enum + Specta regen), then sweep every exhaustive match. Reusing
`SdPath::Cloud` was rejected in the design doc (corrupts service-typed
quota/icon/refresh logic). `NetworkProtocol` comes from VOL-006, hence
the edge, but no backend behavior is needed here.

## Implementation Steps

- [ ] Add the `Network` variant to `SdPath` with `Type` derive; regen
  `ts-client` (`cargo run --bin generate_typescript_types`)
- [ ] Rust match sweep (compiler-enumerated): location validation
  (`locations.validate_path`), search scope resolution, Spacedrop
  targets, file ops dispatch — Network arms resolve through the
  volume's `VolumeBackend` like Cloud arms do
- [ ] TypeScript sweep: `Breadcrumb.parseSdPathSegments`,
  `sdPathToUri` (`sftp://host/path`), mobile explorer hooks —
  every `in SdPath` narrowing site must handle Network
- [ ] `locations.add` accepts Network paths; `locations.validate_path`
  returns reachable/unreachable for them instead of path-risk warnings

## Acceptance Criteria

- [ ] `cargo build` with zero non-exhaustive-match warnings; full
  `tsc -b` clean for touched packages (pre-existing drifts in
  TasksRoute/useSearchFiles excepted)
- [ ] Breadcrumb renders `host/...` segments for a Network path and
  navigates between them
- [ ] A location created on an `sftp://` path lists entries in Explorer
