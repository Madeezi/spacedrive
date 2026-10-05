//! Volume add network operation output types

use crate::volume::{backend::NetworkProtocol, VolumeFingerprint};
use serde::{Deserialize, Serialize};
use specta::Type;

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct VolumeAddNetworkOutput {
	pub fingerprint: VolumeFingerprint,
	pub volume_name: String,
	pub protocol: NetworkProtocol,
}

impl VolumeAddNetworkOutput {
	pub fn new(
		fingerprint: VolumeFingerprint,
		volume_name: String,
		protocol: NetworkProtocol,
	) -> Self {
		Self {
			fingerprint,
			volume_name,
			protocol,
		}
	}
}
