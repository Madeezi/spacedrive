//! Add network volume action
//!
//! This action adds an SFTP network volume (NAS, seedbox, media server)
//! to a library, storing credentials encrypted and creating a virtual
//! volume for indexing. WebDAV support is reserved in the protocol enum
//! but not yet implemented.

use super::output::VolumeAddNetworkOutput;
use crate::{
	context::CoreContext,
	crypto::cloud_credentials::{CloudCredential, CloudCredentialManager},
	infra::action::{error::ActionError, LibraryAction},
	volume::{backend::NetworkProtocol, NetworkBackend, Volume, VolumeBackend, VolumeFingerprint},
};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::{path::PathBuf, sync::Arc};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct VolumeAddNetworkInput {
	pub protocol: NetworkProtocol,
	pub display_name: String,
	pub config: NetworkStorageConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(tag = "type")]
pub enum NetworkStorageConfig {
	Sftp {
		host: String,
		port: u16,
		username: String,
		/// Filesystem path to a private key. `None` selects plain-`ssh`
		/// default identity/agent behavior. No password login exists.
		key_path: Option<String>,
		/// Remote root directory. Defaults to the server login directory.
		root: Option<String>,
	},
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VolumeAddNetworkAction {
	input: VolumeAddNetworkInput,
}

impl VolumeAddNetworkAction {
	pub fn new(input: VolumeAddNetworkInput) -> Self {
		Self { input }
	}
}

impl LibraryAction for VolumeAddNetworkAction {
	type Input = VolumeAddNetworkInput;
	type Output = VolumeAddNetworkOutput;

	fn from_input(input: VolumeAddNetworkInput) -> Result<Self, String> {
		Ok(VolumeAddNetworkAction::new(input))
	}

	async fn execute(
		self,
		library: Arc<crate::library::Library>,
		context: Arc<CoreContext>,
	) -> Result<Self::Output, ActionError> {
		let device_id = context
			.device_manager
			.device_id()
			.map_err(|e| ActionError::InvalidInput(format!("Failed to get device ID: {e}")))?;
		let library_id = library.id();

		let NetworkStorageConfig::Sftp {
			host,
			port,
			username,
			key_path,
			root,
		} = &self.input.config;

		if self.input.protocol != NetworkProtocol::Sftp {
			return Err(ActionError::InvalidInput(
				"Only SFTP is supported for network volumes".to_string(),
			));
		}

		let host = host.trim();
		let username = username.trim();
		if host.is_empty() {
			return Err(ActionError::InvalidInput(
				"SFTP host must not be empty".to_string(),
			));
		}
		if *port == 0 {
			return Err(ActionError::InvalidInput(
				"SFTP port must not be zero".to_string(),
			));
		}
		if username.is_empty() {
			return Err(ActionError::InvalidInput(
				"SFTP username must not be empty".to_string(),
			));
		}

		let key_path_buf: Option<PathBuf> = match key_path {
			Some(k) if !k.trim().is_empty() => Some(PathBuf::from(k.trim())),
			_ => None,
		};

		// Backend construction doubles as the connection test: OpenDAL
		// pools the SSH session, so the first I/O below fails fast on
		// unreachable hosts, bad auth, or unknown host keys.
		let backend =
			NetworkBackend::new_sftp(host, *port, username, key_path_buf.clone(), root.clone())
				.await
				.map_err(|e| {
					ActionError::InvalidInput(format!("Failed to create SFTP backend: {e}"))
				})?;

		backend
			.exists(std::path::Path::new(root.as_deref().unwrap_or("/")))
			.await
			.map_err(|e| {
				ActionError::InvalidInput(format!(
					"SFTP connection test failed (check host, credentials, host key): {e}"
				))
			})?;

		let credential = CloudCredential::new_ssh_key(
			username.to_string(),
			key_path_buf.as_ref().map(|p| p.display().to_string()),
		);

		// Stable identifier always carries the port so :2222 hosts never
		// collide with :22 ones.
		let network_identifier = format!("{host}:{port}");
		let desired_mount_point = format!("sftp://{network_identifier}");
		let mount_point = context
			.volume_manager
			.ensure_unique_mount_point(&desired_mount_point)
			.await;

		let config = serde_json::json!({
			"protocol": "sftp",
			"host": host,
			"port": port,
			"username": username,
			"root": root,
		});
		let fingerprint = VolumeFingerprint::from_network_volume("sftp", &network_identifier);

		let credential_manager = CloudCredentialManager::new(
			context.key_manager.clone(),
			library.db().clone(),
			library_id,
		);

		// Credential rotation: the volume already exists, so only the
		// secret changes. The connection test above already validated it.
		if context
			.volume_manager
			.get_volume(&fingerprint)
			.await
			.is_some()
		{
			credential_manager
				.store_credential(library_id, &fingerprint.0, &credential)
				.await
				.map_err(|e| {
					ActionError::InvalidInput(format!("Failed to store credentials: {e}"))
				})?;

			tracing::info!(
				"Updated credentials for existing network volume {} (fingerprint: {})",
				self.input.display_name,
				fingerprint.0
			);

			return Ok(VolumeAddNetworkOutput::new(
				fingerprint,
				self.input.display_name,
				self.input.protocol,
			));
		}

		let backend_arc: Arc<dyn crate::volume::VolumeBackend> = Arc::new(backend);
		let now = chrono::Utc::now();

		let volume = Volume {
			id: Uuid::new_v4(),
			fingerprint: fingerprint.clone(),
			device_id,
			name: self.input.display_name.clone(),
			library_id: None,
			is_tracked: false,
			mount_point: mount_point.clone(),
			mount_points: vec![mount_point],
			volume_type: crate::volume::types::VolumeType::Network,
			mount_type: crate::volume::types::MountType::Network,
			disk_type: crate::volume::types::DiskType::Unknown,
			file_system: crate::volume::types::FileSystem::Other("SFTP".to_string()),
			total_capacity: 0,
			available_space: 0,
			is_read_only: false,
			is_mounted: true,
			hardware_id: None,
			backend: Some(backend_arc),
			cloud_identifier: Some(network_identifier),
			cloud_config: Some(config),
			apfs_container: None,
			container_volume_id: None,
			path_mappings: Vec::new(),
			is_user_visible: true,
			auto_track_eligible: false,
			read_speed_mbps: None,
			write_speed_mbps: None,
			created_at: now,
			updated_at: now,
			last_seen_at: now,
			total_files: None,
			total_directories: None,
			last_stats_update: None,
			display_name: Some(self.input.display_name.clone()),
			is_favorite: false,
			color: None,
			icon: None,
			error_message: None,
			supports_block_cloning: false,
		};

		credential_manager
			.store_credential(library_id, &fingerprint.0, &credential)
			.await
			.map_err(|e| ActionError::InvalidInput(format!("Failed to store credentials: {e}")))?;

		tracing::info!(
			"Stored credentials for network volume {} in database (library: {}, fingerprint: {})",
			self.input.display_name,
			library_id,
			fingerprint.0
		);

		context
			.volume_manager
			.register_cloud_volume(volume.clone())
			.await;

		let _tracked = context
			.volume_manager
			.track_volume(
				&library,
				&fingerprint,
				Some(self.input.display_name.clone()),
			)
			.await
			.map_err(|e| ActionError::InvalidInput(format!("Volume tracking failed: {e}")))?;

		Ok(VolumeAddNetworkOutput::new(
			fingerprint,
			self.input.display_name,
			self.input.protocol,
		))
	}

	fn action_kind(&self) -> &'static str {
		"volumes.add_network"
	}
}

crate::register_library_action!(VolumeAddNetworkAction, "volumes.add_network");
