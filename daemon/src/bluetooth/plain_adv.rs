//! Parser for the public Apple BLE battery advertisement format, adapted from podpower.

use crate::bluetooth::proto::{Battery, BatteryComponent, BatteryStatus};

const APPLE_MODELS: &[u16] = &[0x0220, 0x0f20, 0x1320, 0x1920, 0x0e20, 0x1420, 0x2420, 0x2720, 0x0a20, 0x1f20];

fn level(raw: u8) -> Option<u8> {
	match raw { 0..=9 => Some(raw * 10 + 5), 10 => Some(100), _ => None }
}

fn battery(component: BatteryComponent, raw: u8, charging: bool) -> Option<Battery> {
	level(raw).map(|level| Battery { component, status: if charging { BatteryStatus::Charging(level) } else { BatteryStatus::Discharging(level) } })
}

/// Decode a known 27-byte Apple manufacturer payload. Unknown models are rejected
/// so encrypted or unrelated advertisements cannot appear as plausible readings.
pub fn decode(data: &[u8]) -> Option<(Vec<Battery>, bool)> {
	if data.len() != 27 { return None; }
	let model = u16::from_be_bytes([data[3], data[4]]);
	if !APPLE_MODELS.contains(&model) { return None; }
	let flip = data[5] & 0x20 == 0;
	let flags = data[7] >> 4;
	let mut out = Vec::with_capacity(3);
	if matches!(model, 0x0a20 | 0x1f20) {
		out.push(battery(BatteryComponent::Left, data[6] & 0x0f, flags & 0x01 != 0)?);
	} else {
		let (left, right) = if flip { (data[6] >> 4, data[6] & 0x0f) } else { (data[6] & 0x0f, data[6] >> 4) };
		let (lf, rf) = if flip { (0x01, 0x02) } else { (0x02, 0x01) };
		if let Some(b) = battery(BatteryComponent::Left, left, flags & lf != 0) { out.push(b); }
		if let Some(b) = battery(BatteryComponent::Right, right, flags & rf != 0) { out.push(b); }
		if let Some(b) = battery(BatteryComponent::Case, data[7] & 0x0f, flags & 0x04 != 0) { out.push(b); }
	}
	Some((out, matches!(model, 0x0a20 | 0x1f20)))
}
