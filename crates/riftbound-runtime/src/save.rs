//! Small versioned local save file for persistent vehicle tuning.

use std::fs;
use std::io;
use std::path::PathBuf;

use riftbound_core::VehicleUpgrades;

const SAVE_FILE: &str = "riftbound-save-v1.txt";
const HEADER: &str = "RIFTBOUND_SAVE_V1";

fn save_path() -> io::Result<PathBuf> {
    Ok(std::env::current_dir()?.join(SAVE_FILE))
}

fn encode(upgrades: VehicleUpgrades) -> String {
    format!(
        "{HEADER}\nengine={}\nhandling={}\n",
        upgrades.engine_level(),
        upgrades.handling_level()
    )
}

fn decode(contents: &str) -> Option<VehicleUpgrades> {
    let mut lines = contents.lines();
    if lines.next()? != HEADER {
        return None;
    }
    let engine = lines.next()?.strip_prefix("engine=")?.parse::<u8>().ok()?;
    let handling = lines.next()?.strip_prefix("handling=")?.parse::<u8>().ok()?;
    if lines.next().is_some() {
        return None;
    }
    VehicleUpgrades::from_levels(engine, handling)
}

/// Loads saved vehicle tuning, falling back to factory settings when unavailable or invalid.
pub fn load_vehicle_upgrades() -> VehicleUpgrades {
    let Ok(path) = save_path() else {
        return VehicleUpgrades::default();
    };
    let Ok(contents) = fs::read_to_string(path) else {
        return VehicleUpgrades::default();
    };
    decode(&contents).unwrap_or_default()
}

/// Saves vehicle tuning through a temporary file before replacing the previous save.
pub fn save_vehicle_upgrades(upgrades: VehicleUpgrades) -> io::Result<()> {
    let path = save_path()?;
    let temporary = path.with_extension("tmp");
    fs::write(&temporary, encode(upgrades))?;
    if let Err(error) = fs::rename(&temporary, &path) {
        let _ = fs::remove_file(&temporary);
        return Err(error);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_round_trip_preserves_both_upgrade_tracks() {
        let upgrades = VehicleUpgrades::from_levels(2, 3).unwrap();
        assert_eq!(decode(&encode(upgrades)), Some(upgrades));
    }

    #[test]
    fn malformed_or_future_save_data_is_rejected() {
        assert_eq!(decode("OTHER_VERSION\nengine=1\nhandling=1\n"), None);
        assert_eq!(decode("RIFTBOUND_SAVE_V1\nengine=4\nhandling=1\n"), None);
        assert_eq!(decode("RIFTBOUND_SAVE_V1\nengine=1\n"), None);
        assert_eq!(decode("RIFTBOUND_SAVE_V1\nengine=1\nhandling=1\nextra\n"), None);
    }
}
