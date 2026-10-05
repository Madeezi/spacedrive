# Network Storage Design

## What is Network Storage?

Network Storage lets a library index files that live behind SFTP or WebDAV
endpoints: NAS boxes, seedboxes, and self-hosted media servers. It follows
the cloud-volume precedent (`volumes.add_cloud`) rather than inventing new
machinery: OpenDAL operators behind the existing `VolumeBackend` trait,
credentials in the encrypted credential store, locations on top, polling
instead of filesystem watching.

## Non-goals

- In-app SMB/NFS client support. OS-mounted shares keep working through
  Add Local Folder; no changes needed there.
- Jellyfin/DLNA/Plex API integrations. Those are media-server APIs, not
  file protocols, and belong in script adapters if anywhere.
- Live watching over the network. `notify` cannot see remote changes;
  network locations rescan on `scan_interval` or manually, like cloud.

## Protocol support

OpenDAL 0.54 (already depended on) ships both services. Only Cargo
features need enabling:

- `services-sftp`: stat, read, write, create_dir, delete, copy, rename,
  list. Key auth only, no password login by upstream design. Host-key
  verification is configurable (`known_hosts_strategy`, default Strict).
- `services-webdav`: stat, read, write, create_dir, delete, copy, rename,
  list. Username/password or bearer token auth.

Consequence: SFTP MVP requires key auth. Users with password-only servers
either deploy a key (document the `ssh-copy-id` step in the modal) or wait
for a future non-OpenDAL backend. WebDAV covers the password case today.

## Decisions

### D1: Addressing — new `SdPath::Network` variant

Reuse of `SdPath::Cloud` was considered and rejected: `Cloud.service`
is `CloudServiceType` (S3, GoogleDrive, ...), and shoehorning
`sftp://host/path` into `{ service, identifier, path }` corrupts the
meaning of every existing Cloud consumer (billing-adjacent quota logic,
service icons, OAuth refresh flows keyed on service type).

New variant:

```rust
{ Network: {
    protocol: NetworkProtocol,   // Sftp | Webdav
    host: String,                // fingerprint-scoped identifier, e.g. "nas.local:22"
    path: String,                // server-relative path
} }
```

Cost: every exhaustive `SdPath` match gains an arm. Known sites:
`Breadcrumb::parseSdPathSegments`, `sdPathToUri`, location validation,
search scope resolution, Spacedrop targets, mobile explorer hooks
(~40 TS call sites, compiler-enumerated via Specta regen).

### D2: Backend shape — `NetworkBackend` beside `CloudBackend`

New `core/src/volume/backend/network.rs` implementing `VolumeBackend`
(`read`, `read_range`, `read_dir`, `metadata`, `exists`, `delete`,
`create_directory`) with `new_sftp` / `new_webdav` constructors mirroring
`CloudBackend::new_s3` et al. `BackendType` gains
`Network(NetworkProtocol)`. The indexer consumes the trait, so discovery,
hashing (ranged reads keep large media cheap), and thumbnails work
without indexer changes.

### D3: Action shape — `volumes.add_network`, mirroring `add_cloud`

New `core/src/ops/volumes/add_network/` (`action.rs`, `input.rs`,
`output.rs`) registered as `volumes.add_network`:

- `NetworkStorageConfig::Sftp { host, port, username, key_path, root }`
  / `::Webdav { endpoint, username, password, root }`
- Build backend, fail closed on connection error (the action doubles as
  the connection test).
- Fingerprint via the existing `VolumeFingerprint::from_network_volume`.
- Volume row uses `VolumeType::Network` / `MountType::Network`, same as
  cloud volumes today. Credentials go through `CloudCredentialManager`
  (rename-worthy later, not now) with a new `CredentialData` variant for
  username+secret and key material.
- Register via `volume_manager.register_cloud_volume` (generalize name
  in passing) then `track_volume`, identical to the cloud flow.

### D4: Freshness — polling only

`Location.scan_interval` already models this (`None` = manual). Network
locations default to a conservative interval (1h proposed); the Overview
volume row shows last-scan time; `is_available=false` on connection
failure with the daemon retrying on next interval. No `notify` integration.

## Frontend contract

- Add Storage modal: replace the hardcoded "Coming Soon" network step
  (`AddStorageModal.tsx`, network-provider branch) with protocol picker
  reusing the existing card grid, then per-protocol config forms
  (SFTP: host/port/username/key; WebDAV: endpoint/username/password),
  Back button and validation-error paths already exist in the component.
- Regenerate `ts-client` (`cargo run --bin generate_typescript_types`):
  new action wire strings, `SdPath::Network`, config inputs.
- `Breadcrumb` and `sdPathToUri` gain Network arms (`sftp://host/path`).

## Verification

- Unit: backend constructors against OpenDAL `Memory` service; credential
  encrypt/decrypt round-trip (pattern exists in `cloud_credentials.rs`).
- Integration: `volumes.add_network` + `locations.add` + indexed listing
  against local `sftp-server` (OpenSSH, always present on macOS dev
  machines) and a `rclone serve webdav` fixture in CI.
- Manual UAT on the Jellyfin host behind SFTP/WebDAV: add, browse,
  rescan, offline (server stopped), credential rotation.

## Phasing

1. SFTP key-auth MVP: D1–D4, manual rescan only, happy path.
2. WebDAV password-auth: second config arm + modal form (~3–4 days
   marginal, reuses everything above).
3. Hardening: reconnect/backoff UX, offline states, polling defaults,
   docs.
