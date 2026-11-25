//! Versioned local profile persistence for Riftbound.

use std::fs;
use std::io;
use std::path::PathBuf;

use riftbound_core::{
    Equipment, EquipmentSlot, Inventory, ItemKind, PlayerProgression, Vec3, VehicleUpgrades,
};

const LEGACY_SAVE_FILE: &str = "riftbound-save-v1.txt";
const HEADER: &str = "RIFTBOUND_SAVE_V1";

/// A separate local campaign profile.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SaveSlot {
    #[default]
    One,
    Two,
    Three,
}

impl SaveSlot {
    /// Returns the next slot in the three-slot cycle.
    pub const fn next(self) -> Self {
        match self {
            Self::One => Self::Two,
            Self::Two => Self::Three,
            Self::Three => Self::One,
        }
    }

    const fn file_name(self) -> &'static str {
        match self {
            Self::One => "riftbound-slot-1.txt",
            Self::Two => "riftbound-slot-2.txt",
            Self::Three => "riftbound-slot-3.txt",
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::One => "SLOT 1",
            Self::Two => "SLOT 2",
            Self::Three => "SLOT 3",
        }
    }
}

pub struct SavedProfile {
    pub upgrades: VehicleUpgrades,
    pub inventory: Inventory,
    pub equipment: Equipment,
    pub progression: PlayerProgression,
    pub checkpoint_active: bool,
    pub player_position: Option<Vec3>,
}

impl Default for SavedProfile {
    fn default() -> Self {
        let mut inventory = Inventory::default();
        inventory.add(ItemKind::ExplorerVest, 1);
        inventory.add(ItemKind::TrailBoots, 1);
        inventory.add(ItemKind::Medkit, 2);
        Self {
            upgrades: VehicleUpgrades::default(),
            inventory,
            equipment: Equipment::default(),
            progression: PlayerProgression::default(),
            checkpoint_active: false,
            player_position: None,
        }
    }
}

fn save_path(slot: SaveSlot) -> io::Result<PathBuf> {
    Ok(std::env::current_dir()?.join(slot.file_name()))
}

fn backup_path(path: &std::path::Path) -> PathBuf {
    path.with_extension("bak")
}

fn read_valid_profile(path: &std::path::Path) -> Option<SavedProfile> {
    let contents = fs::read_to_string(path).ok()?;
    decode(&contents)
}

fn item_code(item: Option<ItemKind>) -> u8 {
    match item {
        None => 0,
        Some(ItemKind::ExplorerVest) => 1,
        Some(ItemKind::TrailBoots) => 2,
        _ => 0,
    }
}

fn decode_equipped(code: u8, expected: EquipmentSlot) -> Option<Option<ItemKind>> {
    match (code, expected) {
        (0, _) => Some(None),
        (1, EquipmentSlot::Body) => Some(Some(ItemKind::ExplorerVest)),
        (2, EquipmentSlot::Feet) => Some(Some(ItemKind::TrailBoots)),
        _ => None,
    }
}

fn encode(profile: &SavedProfile) -> String {
    format!(
        "{HEADER}\nengine={}\nhandling={}\nxp={}\nshards={}\nsalvage={}\nmedkits={}\nvests={}\nboots={}\nvehicle_parts={}\nbody={}\nfeet={}\ncheckpoint={}\nplayer_x={}\nplayer_y={}\nplayer_z={}\n",
        profile.upgrades.engine_level(),
        profile.upgrades.handling_level(),
        profile.progression.total_xp(),
        profile.inventory.count(ItemKind::RiftShard),
        profile.inventory.count(ItemKind::Salvage),
        profile.inventory.count(ItemKind::Medkit),
        profile.inventory.count(ItemKind::ExplorerVest),
        profile.inventory.count(ItemKind::TrailBoots),
        profile.inventory.count(ItemKind::VehiclePart),
        item_code(profile.equipment.item(EquipmentSlot::Body)),
        item_code(profile.equipment.item(EquipmentSlot::Feet)),
        u8::from(profile.checkpoint_active),
        profile
            .player_position
            .map_or_else(|| String::from("-"), |position| position.x.to_string()),
        profile
            .player_position
            .map_or_else(|| String::from("-"), |position| position.y.to_string()),
        profile
            .player_position
            .map_or_else(|| String::from("-"), |position| position.z.to_string()),
    )
}

fn decode(contents: &str) -> Option<SavedProfile> {
    let mut lines = contents.lines();
    if lines.next()? != HEADER {
        return None;
    }
    let mut values = std::collections::BTreeMap::new();
    for line in lines {
        let (key, value) = line.split_once('=')?;
        if values.insert(key, value).is_some() {
            return None;
        }
    }
    if values.len() != 11 && values.len() != 12 && values.len() != 15 {
        return None;
    }
    let number = |key: &str| values.get(key)?.parse::<u32>().ok();
    let engine = u8::try_from(number("engine")?).ok()?;
    let handling = u8::try_from(number("handling")?).ok()?;
    let upgrades = VehicleUpgrades::from_levels(engine, handling)?;
    let xp = values.get("xp")?.parse::<u64>().ok()?;
    let player_position = match (
        values.get("player_x"),
        values.get("player_y"),
        values.get("player_z"),
    ) {
        (None, None, None) | (Some(&"-"), Some(&"-"), Some(&"-")) => None,
        (Some(x), Some(y), Some(z)) => {
            let position = Vec3::new(
                x.parse::<f32>().ok()?,
                y.parse::<f32>().ok()?,
                z.parse::<f32>().ok()?,
            );
            let valid = position.x.is_finite()
                && position.y.is_finite()
                && position.z.is_finite()
                && position.x.abs() <= 100_000.0
                && position.y.abs() <= 100_000.0
                && position.z.abs() <= 100_000.0;
            if !valid {
                return None;
            }
            Some(position)
        }
        _ => return None,
    };
    let checkpoint_active = match values.get("checkpoint").copied() {
        Some("0") | None => false,
        Some("1") => true,
        _ => return None,
    };
    let mut inventory = Inventory::default();
    for (key, kind) in [
        ("shards", ItemKind::RiftShard),
        ("salvage", ItemKind::Salvage),
        ("medkits", ItemKind::Medkit),
        ("vests", ItemKind::ExplorerVest),
        ("boots", ItemKind::TrailBoots),
        ("vehicle_parts", ItemKind::VehiclePart),
    ] {
        let quantity = u16::try_from(number(key)?).ok()?;
        if inventory.add(kind, quantity) != 0 {
            return None;
        }
    }
    let body_code = u8::try_from(number("body")?).ok()?;
    let feet_code = u8::try_from(number("feet")?).ok()?;
    let body = decode_equipped(body_code, EquipmentSlot::Body)?;
    let feet = decode_equipped(feet_code, EquipmentSlot::Feet)?;
    let mut equipment = Equipment::default();
    if let Some(item) = body {
        equipment.equip(item);
    }
    if let Some(item) = feet {
        equipment.equip(item);
    }
    Some(SavedProfile {
        upgrades,
        inventory,
        equipment,
        progression: PlayerProgression::from_total_xp(xp),
        checkpoint_active,
        player_position,
    })
}

/// Loads one slot, falling back to the original save file for legacy Slot 1 users.
pub fn load_profile_from_slot(slot: SaveSlot) -> SavedProfile {
    let Ok(path) = save_path(slot) else {
        return SavedProfile::default();
    };
    read_valid_profile(&path)
        .or_else(|| read_valid_profile(&backup_path(&path)))
        .or_else(|| {
            if slot == SaveSlot::One {
                let legacy = std::env::current_dir().ok()?.join(LEGACY_SAVE_FILE);
                read_valid_profile(&legacy)
            } else {
                None
            }
        })
        .unwrap_or_default()
}

/// Atomically replaces one slot after writing a temporary file.
pub fn save_profile_to_slot(slot: SaveSlot, profile: &SavedProfile) -> io::Result<()> {
    let path = save_path(slot)?;
    let backup = backup_path(&path);
    let temporary = path.with_extension("tmp");
    fs::write(&temporary, encode(profile))?;

    // Only replace the backup with a known-good primary save.
    if read_valid_profile(&path).is_some() {
        if let Err(error) = fs::copy(&path, &backup) {
            let _ = fs::remove_file(&temporary);
            return Err(error);
        }
    }

    // Removing first makes replacement work on platforms where rename
    // cannot overwrite an existing destination. The backup enables recovery
    // if the final rename fails.
    if path.exists() {
        if let Err(error) = fs::remove_file(&path) {
            let _ = fs::remove_file(&temporary);
            return Err(error);
        }
    }
    if let Err(error) = fs::rename(&temporary, &path) {
        let _ = fs::remove_file(&temporary);
        if backup.exists() {
            let _ = fs::copy(&backup, &path);
        }
        return Err(error);
    }
    Ok(())
}

/// Saves to the default campaign slot.
pub fn save_profile(profile: &SavedProfile) -> io::Result<()> {
    save_profile_to_slot(SaveSlot::One, profile)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_slots_cycle_and_use_independent_files() {
        assert_eq!(SaveSlot::One.next(), SaveSlot::Two);
        assert_eq!(SaveSlot::Two.next(), SaveSlot::Three);
        assert_eq!(SaveSlot::Three.next(), SaveSlot::One);
        assert_ne!(SaveSlot::One.file_name(), SaveSlot::Two.file_name());
        assert_ne!(SaveSlot::Two.file_name(), SaveSlot::Three.file_name());
    }

    #[test]
    fn profile_round_trip_preserves_inventory_equipment_xp_and_upgrades() {
        let mut profile = SavedProfile {
            upgrades: VehicleUpgrades::from_levels(2, 3).unwrap(),
            ..SavedProfile::default()
        };
        profile.inventory.add(ItemKind::Salvage, 17);
        profile.inventory.add(ItemKind::Medkit, 2);
        profile.equipment.equip(ItemKind::ExplorerVest);
        profile.progression.award_xp(375);
        profile.checkpoint_active = true;
        profile.player_position = Some(Vec3::new(12.5, 0.2, -8.0));
        let restored = decode(&encode(&profile)).unwrap();
        assert_eq!(restored.upgrades.engine_level(), 2);
        assert_eq!(restored.upgrades.handling_level(), 3);
        assert_eq!(restored.inventory.count(ItemKind::Salvage), 17);
        assert_eq!(restored.inventory.count(ItemKind::Medkit), 2);
        assert_eq!(
            restored.equipment.item(EquipmentSlot::Body),
            Some(ItemKind::ExplorerVest)
        );
        assert_eq!(restored.progression.total_xp(), 375);
        assert!(restored.checkpoint_active);
        assert_eq!(restored.player_position, profile.player_position);
    }

    #[test]
    fn legacy_profile_without_checkpoint_field_remains_loadable() {
        let profile = SavedProfile::default();
        let legacy =
            encode(&profile).replace("checkpoint=0\nplayer_x=-\nplayer_y=-\nplayer_z=-\n", "");
        let restored = decode(&legacy).unwrap();
        assert!(!restored.checkpoint_active);
    }

    #[test]
    fn rejects_non_finite_saved_player_positions() {
        let profile = SavedProfile {
            player_position: Some(Vec3::new(f32::NAN, 0.0, 0.0)),
            ..SavedProfile::default()
        };
        assert!(decode(&encode(&profile)).is_none());
    }

    #[test]
    fn rejects_invalid_or_incomplete_profile_data() {
        assert!(decode("OTHER_VERSION\n").is_none());
        assert!(decode("RIFTBOUND_SAVE_V1\nengine=3\n").is_none());
        assert!(decode("RIFTBOUND_SAVE_V1\nengine=9\n").is_none());
    }
}
