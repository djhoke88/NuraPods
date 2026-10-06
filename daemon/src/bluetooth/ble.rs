use std::sync::Arc;

use aes::Aes128;
use anyhow::{Context, Result, bail};
use bluer::{
	Adapter, AdapterEvent, AddressType, DiscoveryFilter, DiscoveryTransport,
	monitor::{Monitor, MonitorEvent, Pattern, Type, data_type::MANUFACTURER_SPECIFIC_DATA},
};
use bytes::Bytes;
use cbc::Decryptor;
use cipher::{BlockDecryptMut, BlockEncrypt, KeyInit, KeyIvInit, generic_array::GenericArray};
use futures::StreamExt;
use log::{trace, warn};
use tokio::sync::mpsc;

use crate::{bluetooth::proto::ProximityPairingPacket, daemon::DaemonEvent, keystore::KeyStore};

/// Verify a BLE Resolvable Private Address (RPA)
///
/// address layout (little-endian):
/// [ hash(3) | prand(3) ]
pub fn verify_rpa(mut address: [u8; 6], mut irk: Vec<u8>) -> bool {
	address.reverse();
	let mut prand: [u8; 16] = [
		0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, address[5], address[4], address[3],
	];
	irk.reverse();

	let cipher = Aes128::new(GenericArray::from_slice(&irk));
	let mut block = GenericArray::from_mut_slice(&mut prand);
	cipher.encrypt_block(&mut block);

	let correct = [block[15], block[14], block[13]] == [address[0], address[1], address[2]];
	trace!("rpa verify for address {address:?} and irk {irk:?}: {correct}");
	correct
}

pub struct BleClient {
	adapter: Adapter,
	keystore: Arc<KeyStore>,
	events: mpsc::UnboundedSender<DaemonEvent>,
}
impl BleClient {
	pub fn new(
		adapter: Adapter,
		keystore: Arc<KeyStore>,
		events: mpsc::UnboundedSender<DaemonEvent>,
	) -> Self {
		Self {
			adapter,
			keystore,
			events,
		}
	}

	fn decrypt_advertisement(advertisement: Bytes, enc_key: Bytes) -> Result<Bytes> {
		if advertisement.len() < 16 {
			bail!("advertisement too small");
		}
		let enc_key = enc_key.as_array::<16>().context("enc_key incorrect size")?;

		let mut block = [0u8; 16];
		block.copy_from_slice(&advertisement[advertisement.len() - 16..]);

		let iv = [0; 16];
		let mut decryptor = Decryptor::<Aes128>::new(enc_key.into(), &iv.into());
		decryptor.decrypt_block_mut((&mut block).into());

		Ok(Bytes::from(block.to_vec()))
	}

	async fn process_advertisement(
		&self,
		mac: bluer::Address,
		mac_type: AddressType,
		data: Bytes,
	) -> Result<()> {
		if mac_type == AddressType::LeRandom
			&& let Some((addr, keys)) = self
				.keystore
				.find(|keys| verify_rpa(mac.0, keys.irk.to_vec()))
				.await
		{
			let decrypted = Self::decrypt_advertisement(data.clone(), keys.enc_key)
				.context("failed to decrypt advertisement")?;
			self.events
				.send(DaemonEvent::BleAdvertisement(
					addr,
					ProximityPairingPacket::decode(data, decrypted)
						.context("failed to decode advertisement")?,
				))
				.context("failed to send daemon event")?;
			return Ok(());
		}

		if let Some((batteries, single_battery)) = super::plain_adv::decode(&data) {
			self.events
				.send(DaemonEvent::BleAdvertisementPlain(
					mac,
					batteries,
					single_battery,
				))
				.context("failed to send plain BLE battery event")?;
		}
		Ok(())
	}

	async fn monitor_with_discovery(&self) -> Result<()> {
		self.adapter
			.set_discovery_filter(DiscoveryFilter {
				transport: DiscoveryTransport::Le,
				duplicate_data: true,
				..Default::default()
			})
			.await
			.context("failed to configure standard BLE discovery")?;
		let mut devices = self
			.adapter
			.discover_devices_with_changes()
			.await
			.context("failed to start standard BLE discovery")?;
		while let Some(event) = devices.next().await {
			let AdapterEvent::DeviceAdded(address) = event else {
				continue;
			};
			let ret = async {
				let device = self
					.adapter
					.device(address)
					.context("failed to get device")?;
				let mac = device
					.remote_address()
					.await
					.context("failed to get remote address")?;
				let mac_type = device
					.address_type()
					.await
					.context("failed to get address type")?;
				let data_map = device
					.manufacturer_data()
					.await
					.context("failed to get manufacturer data")?;
				if let Some(data) = data_map.and_then(|data| data.get(&0x004C).cloned()) {
					self.process_advertisement(mac, mac_type, Bytes::from(data))
						.await?;
				}
				anyhow::Ok(())
			}
			.await;
			if let Err(error) = ret {
				trace!("standard BLE discovery event for {address} was skipped: {error:#}");
			}
		}
		Ok(())
	}

	pub async fn monitor(self) -> Result<()> {
		let monitor_manager = match self.adapter.monitor().await {
			Ok(manager) => manager,
			Err(error) => {
				warn!("BLE advertisement monitors unavailable ({error}); using standard discovery");
				return self.monitor_with_discovery().await;
			}
		};

		let mut monitor = match monitor_manager
			.register(Monitor {
				monitor_type: Type::OrPatterns,
				patterns: Some(vec![Pattern::new(
					MANUFACTURER_SPECIFIC_DATA,
					0,
					&[0x4C, 0x00, 0x07],
				)]),
				..Default::default()
			})
			.await
		{
			Ok(monitor) => monitor,
			Err(error) => {
				warn!(
					"BLE advertisement monitor registration failed ({error}); using standard discovery"
				);
				return self.monitor_with_discovery().await;
			}
		};

		while let Some(event) = monitor.next().await {
			match event {
				MonitorEvent::DeviceFound(info) => {
					let ret = (async || {
						let device = self
							.adapter
							.device(info.device)
							.context("failed to get device")?;
						let mac = device
							.remote_address()
							.await
							.context("failed to get resolved address")?;
						let mac_type = device
							.address_type()
							.await
							.context("failed to get address type")?;

						trace!("got advertisement from {mac}");

						let data_map = device
							.manufacturer_data()
							.await
							.context("failed to get manufacturer data")?
							.context("device had no manufacturer data")?;
						let data = Bytes::from(
							data_map
								.get(&0x004C)
								.context("device didn't have apple manufacturer data")?
								.to_vec(),
						);
						self.process_advertisement(mac, mac_type, data).await?;

						anyhow::Ok(())
					})()
					.await;

					if let Err(err) = ret {
						warn!(
							"failed to process manufacturer data for {}: {err:?}",
							info.device
						);
					}
				}
				_ => {}
			}
		}

		Ok(())
	}
}
