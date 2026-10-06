//! Network storage backend implementation using OpenDAL
//!
//! This module provides SFTP (and later WebDAV) support for NAS boxes,
//! seedboxes, and self-hosted media servers via Apache OpenDAL.

use async_trait::async_trait;
use bytes::Bytes;
use futures::TryStreamExt;
use opendal::Lister;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use tracing::debug;

use super::{BackendType, NetworkProtocol, RawDirEntry, RawMetadata, VolumeBackend};
use crate::ops::indexing::state::EntryKind;
use crate::volume::error::VolumeError;

/// Network storage backend powered by OpenDAL
///
/// Mirrors `CloudBackend` but addresses hosts over SFTP/WebDAV instead of
/// vendor cloud APIs. SFTP is key-auth only (OpenDAL has no password login
/// by upstream design); the key path is optional and falls back to plain
/// `ssh` default identity/agent behavior when omitted.
///
/// Paths are always server-absolute (`/data/media`): like the S3 backend
/// (where the bucket lives in the operator config, not the path), the
/// operator root stays `/` and every SdPath carries the full path. Passing
/// a scoped root to both the operator AND the path would double it.
#[derive(Debug, Clone)]
pub struct NetworkBackend {
	/// OpenDAL operator for network I/O
	operator: opendal::Operator,

	/// Network protocol for metadata
	protocol: NetworkProtocol,
}

impl NetworkBackend {
	/// Build the OpenDAL endpoint string for an SFTP host.
	///
	/// OpenDAL accepts openssh-style endpoints, where only the
	/// `ssh://host:port` form carries a non-default port.
	pub fn sftp_endpoint(host: &str, port: u16) -> String {
		if port == 22 {
			host.to_string()
		} else {
			format!("ssh://{host}:{port}")
		}
	}

	/// Expand a leading `~` to the user's home directory so modal input
	/// like `~/.ssh/id_ed25519` resolves. Returns the path unchanged
	/// when there is no home to expand to.
	pub fn expand_key_path(path: &Path) -> PathBuf {
		let s = path.to_string_lossy();
		if let Some(rest) = s.strip_prefix("~/") {
			if let Some(home) = dirs::home_dir() {
				return home.join(rest);
			}
		}
		path.to_path_buf()
	}

	/// Create a new network backend for SFTP
	///
	/// `key_path` is a filesystem path to a private key (`~` expands to
	/// the home directory). Pass `None` to use default SSH identity
	/// files and any running ssh-agent, exactly like `ssh user@host`
	/// in a terminal.
	pub async fn new_sftp(
		host: impl AsRef<str>,
		port: u16,
		username: impl AsRef<str>,
		key_path: Option<PathBuf>,
	) -> Result<Self, VolumeError> {
		let host = host.as_ref().trim();
		let username = username.as_ref().trim();
		let key_path = key_path.map(|k| Self::expand_key_path(&k));

		if host.is_empty() {
			return Err(VolumeError::Platform(
				"SFTP host must not be empty".to_string(),
			));
		}
		if port == 0 {
			return Err(VolumeError::Platform(
				"SFTP port must not be zero".to_string(),
			));
		}
		if username.is_empty() {
			return Err(VolumeError::Platform(
				"SFTP username must not be empty".to_string(),
			));
		}
		if let Some(key) = &key_path {
			if !key.exists() {
				return Err(VolumeError::Platform(format!(
					"SFTP key file not found: {}",
					key.display()
				)));
			}
		}

		// The SSH multiplexer needs a unix socket under the session state
		// dir, capped at ~104 bytes by the OS. A long XDG_STATE_HOME makes
		// every connection fail after the pool timeout instead of at once.
		if let Some(state_home) = std::env::var_os("XDG_STATE_HOME") {
			let state_home = state_home.to_string_lossy();
			if state_home.len() > 60 {
				return Err(VolumeError::Platform(
					"XDG_STATE_HOME is too long for SSH multiplexing; unset it for the daemon process (exported control sockets are capped at ~104 bytes)".to_string(),
				));
			}
		}

		let mut builder = opendal::services::Sftp::default()
			.endpoint(&Self::sftp_endpoint(host, port))
			.user(username);

		// Explicit root: without it OpenDAL resolves paths relative to the
		// SSH login directory instead of the filesystem root.
		builder = builder.root("/");

		if let Some(key) = &key_path {
			let expanded = Self::expand_key_path(key);
			let key_str = expanded.to_str().ok_or_else(|| {
				VolumeError::Platform("SFTP key path is not valid UTF-8".to_string())
			})?;
			builder = builder.key(key_str);
		}

		let operator = opendal::Operator::new(builder)
			.map_err(|e| VolumeError::Platform(format!("Failed to create SFTP operator: {e}")))?
			.finish();

		Ok(Self {
			operator,
			protocol: NetworkProtocol::Sftp,
		})
	}

	/// Create a network backend from a pre-configured OpenDAL operator
	pub fn from_operator(operator: opendal::Operator, protocol: NetworkProtocol) -> Self {
		Self { operator, protocol }
	}
}

impl NetworkBackend {
	/// Convert path to remote storage path (removes leading /)
	fn to_remote_path(&self, path: &Path) -> String {
		path.to_str()
			.unwrap_or("")
			.trim_start_matches('/')
			.to_string()
	}
}

#[async_trait]
impl VolumeBackend for NetworkBackend {
	async fn read(&self, path: &Path) -> Result<Bytes, VolumeError> {
		let remote_path = self.to_remote_path(path);
		debug!("NetworkBackend::read: {}", remote_path);

		let data = self
			.operator
			.read(&remote_path)
			.await
			.map_err(|e| VolumeError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;

		Ok(data.to_bytes())
	}

	async fn read_range(&self, path: &Path, range: Range<u64>) -> Result<Bytes, VolumeError> {
		let remote_path = self.to_remote_path(path);
		debug!(
			"NetworkBackend::read_range: {} ({}..{})",
			remote_path, range.start, range.end
		);

		let data = self
			.operator
			.read_with(&remote_path)
			.range(range.start..range.end)
			.await
			.map_err(|e| VolumeError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;

		Ok(data.to_bytes())
	}

	async fn write(&self, path: &Path, data: Bytes) -> Result<(), VolumeError> {
		let remote_path = self.to_remote_path(path);
		debug!(
			"NetworkBackend::write: {} ({} bytes)",
			remote_path,
			data.len()
		);

		self.operator
			.write(&remote_path, data)
			.await
			.map_err(|e| VolumeError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;

		Ok(())
	}

	async fn read_dir(&self, path: &Path) -> Result<Vec<RawDirEntry>, VolumeError> {
		// SFTP listers only enumerate children when the directory path
		// ends with '/'; without it the server returns the dir itself.
		let mut remote_path = self.to_remote_path(path);
		if !remote_path.ends_with('/') {
			remote_path.push('/');
		}
		debug!("NetworkBackend::read_dir: {}", remote_path);

		let mut entries = Vec::new();
		let lister = self
			.operator
			.lister(&remote_path)
			.await
			.map_err(|e| VolumeError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;

		let mut lister = lister;
		while let Some(entry_result) = lister.try_next().await.transpose() {
			let entry = entry_result
				.map_err(|e| VolumeError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;

			let metadata = entry.metadata();
			let name = entry
				.name()
				.trim_end_matches('/')
				.split('/')
				.last()
				.unwrap_or(entry.name())
				.to_string();

			let kind = if metadata.is_dir() {
				EntryKind::Directory
			} else {
				EntryKind::File
			};

			entries.push(RawDirEntry {
				name,
				kind,
				size: metadata.content_length(),
				modified: metadata.last_modified().map(|t| {
					SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(t.timestamp() as u64)
				}),
				inode: None, // Remote storage doesn't expose inodes
			});
		}

		Ok(entries)
	}

	async fn metadata(&self, path: &Path) -> Result<RawMetadata, VolumeError> {
		let remote_path = self.to_remote_path(path);
		debug!("NetworkBackend::metadata: {}", remote_path);

		let metadata = self
			.operator
			.stat(&remote_path)
			.await
			.map_err(|e| VolumeError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;

		let kind = if metadata.is_dir() {
			EntryKind::Directory
		} else {
			EntryKind::File
		};

		let modified = metadata
			.last_modified()
			.map(|t| SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(t.timestamp() as u64));

		Ok(RawMetadata {
			kind,
			size: metadata.content_length(),
			modified,
			created: None, // SFTP servers rarely provide creation time
			accessed: None,
			inode: None,       // Remote storage doesn't expose inodes
			permissions: None, // Remote permission bits aren't portable
		})
	}

	async fn exists(&self, path: &Path) -> Result<bool, VolumeError> {
		let remote_path = self.to_remote_path(path);
		// OpenDAL doesn't have a direct exists() method, use stat() instead
		match self.operator.stat(&remote_path).await {
			Ok(_) => Ok(true),
			Err(_) => Ok(false),
		}
	}

	async fn delete(&self, path: &Path) -> Result<(), VolumeError> {
		let remote_path = self.to_remote_path(path);
		debug!("NetworkBackend::delete: {}", remote_path);

		let metadata = self
			.operator
			.stat(&remote_path)
			.await
			.map_err(|e| VolumeError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;

		if metadata.is_dir() {
			self.operator
				.remove_all(&remote_path)
				.await
				.map_err(|e| VolumeError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;
		} else {
			self.operator
				.delete(&remote_path)
				.await
				.map_err(|e| VolumeError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;
		}

		Ok(())
	}

	async fn create_directory(&self, path: &Path, recursive: bool) -> Result<(), VolumeError> {
		let mut remote_path = self.to_remote_path(path);
		debug!(
			"NetworkBackend::create_directory: {} (recursive: {})",
			remote_path, recursive
		);

		if !remote_path.ends_with('/') {
			remote_path.push('/');
		}

		self.operator
			.create_dir(&remote_path)
			.await
			.map_err(|e| VolumeError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;

		Ok(())
	}

	fn is_local(&self) -> bool {
		false
	}

	fn backend_type(&self) -> BackendType {
		BackendType::Network(self.protocol)
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn sftp_endpoint_uses_default_port_form() {
		assert_eq!(NetworkBackend::sftp_endpoint("nas.local", 22), "nas.local");
	}

	#[test]
	fn sftp_endpoint_carries_custom_port() {
		assert_eq!(
			NetworkBackend::sftp_endpoint("nas.local", 2222),
			"ssh://nas.local:2222"
		);
	}

	#[test]
	fn expand_key_path_resolves_home() {
		let expanded = NetworkBackend::expand_key_path(Path::new("~/.ssh/id_ed25519"));
		if let Some(home) = dirs::home_dir() {
			assert_eq!(expanded, home.join(".ssh/id_ed25519"));
		} else {
			assert_eq!(expanded, PathBuf::from("~/.ssh/id_ed25519"));
		}

		assert_eq!(
			NetworkBackend::expand_key_path(Path::new("/etc/ssh/key")),
			PathBuf::from("/etc/ssh/key")
		);
	}
	#[tokio::test]
	async fn sftp_rejects_empty_host() {
		let err = NetworkBackend::new_sftp("", 22, "media", None)
			.await
			.unwrap_err();
		assert!(matches!(err, VolumeError::Platform(_)));
	}

	#[tokio::test]
	async fn sftp_rejects_zero_port() {
		let err = NetworkBackend::new_sftp("nas.local", 0, "media", None)
			.await
			.unwrap_err();
		assert!(matches!(err, VolumeError::Platform(_)));
	}

	#[tokio::test]
	async fn sftp_rejects_empty_username() {
		let err = NetworkBackend::new_sftp("nas.local", 22, "  ", None)
			.await
			.unwrap_err();
		assert!(matches!(err, VolumeError::Platform(_)));
	}

	#[tokio::test]
	async fn sftp_rejects_missing_key_file() {
		let err = NetworkBackend::new_sftp(
			"nas.local",
			22,
			"media",
			Some(PathBuf::from("/does/not/exist_ed25519")),
		)
		.await
		.unwrap_err();
		assert!(matches!(err, VolumeError::Platform(_)));
	}

	#[test]
	fn network_protocol_scheme_roundtrip() {
		assert_eq!(NetworkProtocol::Sftp.scheme(), "sftp");
		assert_eq!(NetworkProtocol::Webdav.scheme(), "webdav");
		assert_eq!(
			NetworkProtocol::from_scheme("sftp"),
			Some(NetworkProtocol::Sftp)
		);
		assert_eq!(NetworkProtocol::from_scheme("ftp"), None);
	}

	// Live-server test, ignored by default. Requires an SFTP server:
	// SFTP_HOST, SFTP_USER, SFTP_KEY (optional path).
	#[tokio::test]
	#[ignore]
	async fn test_network_backend_sftp_live() {
		let host = std::env::var("SFTP_HOST").expect("SFTP_HOST must be set to run this test");
		let user = std::env::var("SFTP_USER").unwrap_or_else(|_| "root".to_string());
		let key = std::env::var("SFTP_KEY").ok().map(PathBuf::from);

		let backend = NetworkBackend::new_sftp(&host, 22, &user, key)
			.await
			.unwrap();

		let entries = backend.read_dir(Path::new("/")).await.unwrap();
		assert!(entries.iter().all(|e| !e.name.is_empty()));

		assert!(backend.backend_type() == BackendType::Network(NetworkProtocol::Sftp));
		assert!(!backend.is_local());
	}
}
