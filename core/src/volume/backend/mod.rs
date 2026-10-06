//! Volume backend implementations
//!
//! This module provides the backend abstraction layer for heterogeneous storage I/O.

use async_trait::async_trait;
use bytes::Bytes;
use std::fmt::Debug;
use std::ops::Range;
use std::path::Path;
use std::time::SystemTime;

use crate::ops::indexing::state::EntryKind;
use crate::volume::error::VolumeError;

pub mod cloud;
pub mod local;
pub mod network;

pub use cloud::CloudBackend;
pub use local::LocalBackend;
pub use network::NetworkBackend;

/// Minimal I/O backend trait for volume operations
///
/// This trait provides only low-level filesystem operations. All domain logic
/// (Entry creation, content identification, etc.) is handled by existing
/// Spacedrive infrastructure that consumes these raw operations.
#[async_trait]
pub trait VolumeBackend: Send + Sync + Debug {
	/// Read entire file content
	async fn read(&self, path: &Path) -> Result<Bytes, VolumeError>;

	/// Read specific byte range from file (critical for cloud efficiency)
	async fn read_range(&self, path: &Path, range: Range<u64>) -> Result<Bytes, VolumeError>;

	/// Write file content
	async fn write(&self, path: &Path, data: Bytes) -> Result<(), VolumeError>;

	/// List directory entries (returns minimal metadata)
	async fn read_dir(&self, path: &Path) -> Result<Vec<RawDirEntry>, VolumeError>;

	/// Get file/directory metadata
	async fn metadata(&self, path: &Path) -> Result<RawMetadata, VolumeError>;

	/// Check if path exists (optimized when possible)
	async fn exists(&self, path: &Path) -> Result<bool, VolumeError>;

	/// Delete file or directory
	async fn delete(&self, path: &Path) -> Result<(), VolumeError>;

	/// Create a directory at the specified path
	async fn create_directory(&self, path: &Path, recursive: bool) -> Result<(), VolumeError>;

	/// Backend identification (used to optimize operations)
	fn is_local(&self) -> bool;

	/// Get backend type identifier
	fn backend_type(&self) -> BackendType;
}

/// Strip a `scheme://host/` URI prefix down to the server-relative path.
///
/// Discovery threads full URIs (`sftp://host/data/media/movies`) back
/// through listing calls, while fresh paths arrive bare (`/data/media`).
/// Both forms must resolve identically or recursion silently lists the
/// wrong (nonexistent, swallowed-empty) location.
pub(crate) fn strip_uri_prefix(path: &str) -> &str {
	let path = path.trim_start_matches('/');
	match path.split_once("://") {
		Some((_, rest)) => match rest.split_once('/') {
			Some((_, p)) => p,
			None => "",
		},
		None => path,
	}
}
/// Backend type identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendType {
	Local,
	Cloud(CloudServiceType),
	Network(NetworkProtocol),
}

/// Network protocol for remote volumes addressed over SFTP/WebDAV
#[derive(
	Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize, specta::Type,
)]
pub enum NetworkProtocol {
	#[serde(rename = "sftp")]
	Sftp,
	#[serde(rename = "webdav")]
	Webdav,
}

impl NetworkProtocol {
	/// Get the URI scheme for this protocol
	pub fn scheme(&self) -> &'static str {
		match self {
			Self::Sftp => "sftp",
			Self::Webdav => "webdav",
		}
	}

	/// Parse network protocol from URI scheme
	/// Returns None if the scheme doesn't match any known protocol
	pub fn from_scheme(scheme: &str) -> Option<Self> {
		match scheme {
			"sftp" => Some(Self::Sftp),
			"webdav" => Some(Self::Webdav),
			_ => None,
		}
	}
}

/// Cloud service type identifier
#[derive(
	Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize, specta::Type,
)]
pub enum CloudServiceType {
	#[serde(rename = "s3")]
	S3,
	#[serde(rename = "gdrive")]
	GoogleDrive,
	#[serde(rename = "dropbox")]
	Dropbox,
	#[serde(rename = "onedrive")]
	OneDrive,
	#[serde(rename = "gcs")]
	GoogleCloudStorage,
	#[serde(rename = "azblob")]
	AzureBlob,
	#[serde(rename = "b2")]
	BackblazeB2,
	#[serde(rename = "wasabi")]
	Wasabi,
	#[serde(rename = "spaces")]
	DigitalOceanSpaces,
	#[serde(rename = "cloud")]
	Other,
}

impl CloudServiceType {
	/// Get the URI scheme for this cloud service
	/// Used for service-native addressing (e.g., "s3://bucket/path")
	pub fn scheme(&self) -> &'static str {
		match self {
			Self::S3 => "s3",
			Self::GoogleDrive => "gdrive",
			Self::OneDrive => "onedrive",
			Self::Dropbox => "dropbox",
			Self::AzureBlob => "azblob",
			Self::GoogleCloudStorage => "gcs",
			Self::BackblazeB2 => "b2",
			Self::Wasabi => "wasabi",
			Self::DigitalOceanSpaces => "spaces",
			Self::Other => "cloud",
		}
	}

	/// Parse cloud service type from URI scheme
	/// Returns None if the scheme doesn't match any known service
	pub fn from_scheme(scheme: &str) -> Option<Self> {
		match scheme {
			"s3" => Some(Self::S3),
			"gdrive" => Some(Self::GoogleDrive),
			"onedrive" => Some(Self::OneDrive),
			"dropbox" => Some(Self::Dropbox),
			"azblob" => Some(Self::AzureBlob),
			"gcs" => Some(Self::GoogleCloudStorage),
			"b2" => Some(Self::BackblazeB2),
			"wasabi" => Some(Self::Wasabi),
			"spaces" => Some(Self::DigitalOceanSpaces),
			_ => None,
		}
	}
}

/// Raw directory entry returned by volume backends
#[derive(Debug, Clone)]
pub struct RawDirEntry {
	pub name: String,
	pub kind: EntryKind,
	pub size: u64,
	pub modified: Option<SystemTime>,
	pub inode: Option<u64>,
}

/// Raw metadata returned by volume backends
#[derive(Debug, Clone)]
pub struct RawMetadata {
	pub kind: EntryKind,
	pub size: u64,
	pub modified: Option<SystemTime>,
	pub created: Option<SystemTime>,
	pub accessed: Option<SystemTime>,
	pub inode: Option<u64>,
	/// Unix permission bits (mode), None for cloud backends or Windows
	pub permissions: Option<u32>,
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn strip_uri_prefix_handles_all_path_forms() {
		assert_eq!(strip_uri_prefix("/data/media"), "data/media");
		assert_eq!(strip_uri_prefix("data/media"), "data/media");
		assert_eq!(
			strip_uri_prefix("sftp://10.0.0.101:22/data/media"),
			"data/media"
		);
		assert_eq!(strip_uri_prefix("s3://bucket/photos"), "photos");
		assert_eq!(strip_uri_prefix("sftp://host"), "");
		assert_eq!(strip_uri_prefix(""), "");
	}
}
