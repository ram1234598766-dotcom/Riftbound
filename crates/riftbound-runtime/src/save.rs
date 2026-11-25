//! Versioned local profile persistence for Riftbound.

use std::fs;
use std::io;
use std::path::PathBuf;

use riftbound_core::{
    Equipment, EquipmentSlot, Inventory, ItemKind, PlayerProgression, VehicleUpgrades,
};

const SAVE_FILE: &str = "riftbound-save-v1.txt";
const HEADER: &str = "RIFTBOUND_SAVE_V1";

pub struct SavedProfile {
    pub upgrades: VehicleUpgrades,
    pub inventory: Inventory,
    pub equipment: Equipment,
    pub progression: PlayerProgression,
}

impl Default for SavedProfile {
    fn default() -> Self {
        Self {
            upgrades: VehicleUpgrades::default(),
            inventory: Inventory::default(),
            equipment: Equipment::default(),
            progression: PlayerProgression::default(),
        }
    }
}

fn save_path() -> io::Result<PathBuf> {
    Ok(std::env::current_dir()?.join(SAVE_FILE))
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
        "{HEADER}\nengine={}\nhandling={}\nxp={}\nshards={}\nsalvage={}\nmedkits={}\nvests={}\nboots={}\nvehicle_parts={}\nbody={}\nfeet={}\n",
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
    if values.len() != 11 {
        return None;
    }
    let number = |key: &str| values.get(key)?.parse::<u32>().ok();
    let engine = u8::try_from(number("engine")?).ok()?;
    let handling = u8::try_from(number("handling")?).ok()?;
    let upgrades = VehicleUpgrades::from_levels(engine, handling)?;
    let xp = values.get("xp")?.parse::<u64>().ok()?;
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
    if let Some(item) = body { equipment.equip(item); }
    if let Some(item) = feet { equipment.equip(item); }
    Some(SavedProfile {
        upgrades,
        inventory,
        equipment,
        progression: PlayerProgression::from_total_xp(xp),
    })
}

/// Loads a profile, using a clean profile when no valid save exists.
pub fn load_profile() -> SavedProfile {
    let Ok(path) = save_path() else { return SavedProfile::default(); };
    let Ok(contents) = fs::read_to_string(path) else { return SavedProfile::default(); };
    decode(&contents).unwrap_or_default()
}

/// Atomically replaces the profile save after writing a temporary file.
pub fn save_profile(profile: &SavedProfile) -> io::Result<()> {
    let path = save_path()?;
    let temporary = path.with_extension("tmp");
    fs::write(&temporary, encode(profile))?;
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
    fn profile_round_trip_preserves_inventory_equipment_xp_and_upgrades() {
        let mut profile = SavedProfile::default();
        profile.upgrades = VehicleUpgrades::from_levels(2, 3).unwrap();
        profile.inventory.add(ItemKind::Salvage, 17);
        profile.inventory.add(ItemKind::Medkit, 2);
        profile.equipment.equip(ItemKind::ExplorerVest);
        profile.progression.award_xp(375);
        let restored = decode(&encode(&profile)).unwrap();
        assert_eq!(restored.upgrades.engine_level(), 2);
        assert_eq!(restored.upgrades.handling_level(), 3);
        assert_eq!(restored.inventory.count(ItemKind::Salvage), 17);
        assert_eq!(restored.inventory.count(ItemKind::Medkit), 2);
        assert_eq!(restored.equipment.item(EquipmentSlot::Body), Some(ItemKind::ExplorerVest));
        assert_eq!(restored.progression.total_xp(), 375);
    }

    #[test]
    fn rejects_invalid_or_incomplete_profile_data() {
        assert!(decode("OTHER_VERSION\n").is_none());
        assert!(decode("RIFTBOUND_SAVE_V1\nengine=3\n").is_none());
        assert!(decode("RIFTBOUND_SAVE_V1\nengine=9\n").is_none());
    }
}
