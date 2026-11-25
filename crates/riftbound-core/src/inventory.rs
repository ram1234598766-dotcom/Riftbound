//! Compact item inventory and equipment primitives for RPG gameplay.

/// Stable item categories used by quests, loot, crafting, and equipment.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ItemKind {
    /// Quest collectible recovered from rift sites.
    RiftShard,
    /// Common material recovered while exploring.
    Salvage,
    /// Consumable that restores a fixed amount of health.
    Medkit,
    /// Protective chest equipment.
    ExplorerVest,
    /// Footwear tuned for traversal.
    TrailBoots,
    /// Upgrade component for vehicles.
    VehiclePart,
}

impl ItemKind {
    /// Maximum quantity allowed in one inventory stack.
    #[must_use]
    pub const fn max_stack(self) -> u16 {
        match self {
            Self::RiftShard | Self::Salvage | Self::Medkit | Self::VehiclePart => 99,
            Self::ExplorerVest | Self::TrailBoots => 1,
        }
    }

    /// Equipment slot accepted by this item, if it is wearable.
    #[must_use]
    pub const fn equipment_slot(self) -> Option<EquipmentSlot> {
        match self {
            Self::ExplorerVest => Some(EquipmentSlot::Body),
            Self::TrailBoots => Some(EquipmentSlot::Feet),
            _ => None,
        }
    }

    /// Flat damage reduction granted while equipped.
    #[must_use]
    pub const fn armor_bonus(self) -> u8 {
        match self {
            Self::ExplorerVest => 8,
            _ => 0,
        }
    }

    /// Traversal speed multiplier bonus while equipped.
    #[must_use]
    pub const fn movement_bonus(self) -> f32 {
        match self {
            Self::TrailBoots => 0.08,
            _ => 0.0,
        }
    }
}

/// A quantity of one item category.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ItemStack {
    kind: ItemKind,
    quantity: u16,
}

impl ItemStack {
    /// Creates a non-empty stack within the item's stack limit.
    #[must_use]
    pub const fn new(kind: ItemKind, quantity: u16) -> Option<Self> {
        if quantity == 0 || quantity > kind.max_stack() {
            None
        } else {
            Some(Self { kind, quantity })
        }
    }

    /// Item category in this stack.
    #[must_use]
    pub const fn kind(self) -> ItemKind {
        self.kind
    }

    /// Number of items in this stack.
    #[must_use]
    pub const fn quantity(self) -> u16 {
        self.quantity
    }
}

/// Fixed equipment locations supported by the explorer character.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EquipmentSlot {
    /// Torso protection.
    Body,
    /// Traversal footwear.
    Feet,
}

/// Small slot-based inventory with deterministic stacking behavior.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Inventory {
    slots: Vec<Option<ItemStack>>,
}

impl Default for Inventory {
    fn default() -> Self {
        Self::new(12)
    }
}

impl Inventory {
    /// Creates an inventory with the requested number of slots.
    #[must_use]
    pub fn new(slot_count: usize) -> Self {
        Self {
            slots: vec![None; slot_count],
        }
    }

    /// Adds as many items as possible and returns the quantity that did not fit.
    pub fn add(&mut self, kind: ItemKind, quantity: u16) -> u16 {
        let mut remaining = quantity;
        if remaining == 0 {
            return 0;
        }
        let limit = kind.max_stack();
        for stack in self
            .slots
            .iter_mut()
            .flatten()
            .filter(|stack| stack.kind == kind)
        {
            let room = limit - stack.quantity;
            let added = room.min(remaining);
            stack.quantity += added;
            remaining -= added;
            if remaining == 0 {
                return 0;
            }
        }
        for slot in &mut self.slots {
            if slot.is_none() {
                let added = limit.min(remaining);
                *slot = Some(ItemStack {
                    kind,
                    quantity: added,
                });
                remaining -= added;
                if remaining == 0 {
                    return 0;
                }
            }
        }
        remaining
    }

    /// Removes up to the requested quantity and returns how many were removed.
    pub fn remove(&mut self, kind: ItemKind, quantity: u16) -> u16 {
        let mut remaining = quantity;
        for slot in &mut self.slots {
            if let Some(stack) = slot {
                if stack.kind == kind {
                    let removed = stack.quantity.min(remaining);
                    stack.quantity -= removed;
                    remaining -= removed;
                    if stack.quantity == 0 {
                        *slot = None;
                    }
                    if remaining == 0 {
                        break;
                    }
                }
            }
        }
        quantity - remaining
    }

    /// Moves one compatible item from inventory into its equipment slot.
    ///
    /// The operation is transactional: an existing equipped item is returned
    /// to inventory, and no item is lost if the requested item is unavailable.
    pub fn equip_from_inventory(&mut self, equipment: &mut Equipment, item: ItemKind) -> bool {
        if item.equipment_slot().is_none() || self.count(item) == 0 {
            return false;
        }
        self.remove(item, 1);
        if let Some(replaced) = equipment.equip(item) {
            let overflow = self.add(replaced, 1);
            debug_assert_eq!(
                overflow, 0,
                "replacing equipment must free inventory capacity"
            );
        }
        true
    }

    /// Moves equipped gear back into inventory when there is room.
    pub fn unequip_to_inventory(&mut self, equipment: &mut Equipment, slot: EquipmentSlot) -> bool {
        let Some(item) = equipment.item(slot) else {
            return false;
        };
        if self.add(item, 1) != 0 {
            return false;
        }
        let removed = equipment.unequip(slot);
        debug_assert_eq!(removed, Some(item));
        true
    }

    /// Total quantity of an item across all stacks.
    #[must_use]
    pub fn count(&self, kind: ItemKind) -> u32 {
        self.slots
            .iter()
            .flatten()
            .filter(|stack| stack.kind == kind)
            .map(|stack| u32::from(stack.quantity))
            .sum()
    }

    /// Number of occupied inventory slots.
    #[must_use]
    pub fn occupied_slots(&self) -> usize {
        self.slots.iter().filter(|slot| slot.is_some()).count()
    }

    /// Total inventory capacity in slots.
    #[must_use]
    pub fn capacity(&self) -> usize {
        self.slots.len()
    }

    /// Read-only view of the inventory slots.
    #[must_use]
    pub fn slots(&self) -> &[Option<ItemStack>] {
        &self.slots
    }
}

/// Equipped items for the player, separate from carried inventory.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Equipment {
    body: Option<ItemKind>,
    feet: Option<ItemKind>,
}

impl Equipment {
    /// Equips a compatible item, returning the item previously in that slot.
    pub fn equip(&mut self, item: ItemKind) -> Option<ItemKind> {
        match item.equipment_slot()? {
            EquipmentSlot::Body => self.body.replace(item),
            EquipmentSlot::Feet => self.feet.replace(item),
        }
    }

    /// Removes and returns the item in a slot.
    pub fn unequip(&mut self, slot: EquipmentSlot) -> Option<ItemKind> {
        match slot {
            EquipmentSlot::Body => self.body.take(),
            EquipmentSlot::Feet => self.feet.take(),
        }
    }

    /// Item currently equipped in a slot.
    #[must_use]
    pub const fn item(self, slot: EquipmentSlot) -> Option<ItemKind> {
        match slot {
            EquipmentSlot::Body => self.body,
            EquipmentSlot::Feet => self.feet,
        }
    }

    /// Combined flat armor value from all equipped items.
    #[must_use]
    pub const fn armor_bonus(self) -> u8 {
        match self.body {
            Some(item) => item.armor_bonus(),
            None => 0,
        }
    }

    /// Combined movement multiplier bonus from equipped items.
    #[must_use]
    pub const fn movement_bonus(self) -> f32 {
        let body_bonus = match self.body {
            Some(item) => item.movement_bonus(),
            None => 0.0,
        };
        let feet_bonus = match self.feet {
            Some(item) => item.movement_bonus(),
            None => 0.0,
        };
        body_bonus + feet_bonus
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn item_stacks_enforce_quantity_limits() {
        assert!(ItemStack::new(ItemKind::RiftShard, 99).is_some());
        assert!(ItemStack::new(ItemKind::RiftShard, 100).is_none());
        assert!(ItemStack::new(ItemKind::ExplorerVest, 2).is_none());
        assert!(ItemStack::new(ItemKind::Salvage, 0).is_none());
    }

    #[test]
    fn inventory_stacks_items_and_reports_overflow() {
        let mut inventory = Inventory::new(2);
        assert_eq!(inventory.add(ItemKind::RiftShard, 150), 0);
        assert_eq!(inventory.count(ItemKind::RiftShard), 150);
        assert_eq!(inventory.occupied_slots(), 2);
        assert_eq!(inventory.add(ItemKind::Medkit, 1), 1);
    }

    #[test]
    fn removal_spans_stacks_and_frees_empty_slots() {
        let mut inventory = Inventory::new(2);
        inventory.add(ItemKind::Salvage, 150);
        assert_eq!(inventory.remove(ItemKind::Salvage, 120), 120);
        assert_eq!(inventory.count(ItemKind::Salvage), 30);
        assert_eq!(inventory.remove(ItemKind::Salvage, 50), 30);
        assert_eq!(inventory.occupied_slots(), 0);
    }

    #[test]
    fn equipment_accepts_only_compatible_items_and_returns_replaced_item() {
        let mut equipment = Equipment::default();
        assert_eq!(equipment.equip(ItemKind::Medkit), None);
        assert_eq!(equipment.equip(ItemKind::ExplorerVest), None);
        assert_eq!(
            equipment.equip(ItemKind::ExplorerVest),
            Some(ItemKind::ExplorerVest)
        );
        assert_eq!(equipment.armor_bonus(), 8);
        assert_eq!(
            equipment.unequip(EquipmentSlot::Body),
            Some(ItemKind::ExplorerVest)
        );
        assert_eq!(equipment.armor_bonus(), 0);
    }

    #[test]
    fn equipping_moves_items_and_returns_replaced_gear() {
        let mut inventory = Inventory::new(2);
        let mut equipment = Equipment::default();
        inventory.add(ItemKind::ExplorerVest, 1);
        inventory.add(ItemKind::TrailBoots, 1);

        assert!(inventory.equip_from_inventory(&mut equipment, ItemKind::ExplorerVest));
        assert_eq!(inventory.count(ItemKind::ExplorerVest), 0);
        assert_eq!(
            equipment.item(EquipmentSlot::Body),
            Some(ItemKind::ExplorerVest)
        );

        inventory.add(ItemKind::ExplorerVest, 1);
        assert!(inventory.equip_from_inventory(&mut equipment, ItemKind::ExplorerVest));
        assert_eq!(inventory.count(ItemKind::ExplorerVest), 1);
        assert_eq!(
            equipment.item(EquipmentSlot::Body),
            Some(ItemKind::ExplorerVest)
        );
    }

    #[test]
    fn unequipping_requires_inventory_space() {
        let mut inventory = Inventory::new(1);
        let mut equipment = Equipment::default();
        equipment.equip(ItemKind::TrailBoots);
        inventory.add(ItemKind::Salvage, 99);

        assert!(!inventory.unequip_to_inventory(&mut equipment, EquipmentSlot::Feet));
        assert_eq!(
            equipment.item(EquipmentSlot::Feet),
            Some(ItemKind::TrailBoots)
        );

        inventory.remove(ItemKind::Salvage, 99);
        assert!(inventory.unequip_to_inventory(&mut equipment, EquipmentSlot::Feet));
        assert_eq!(equipment.item(EquipmentSlot::Feet), None);
        assert_eq!(inventory.count(ItemKind::TrailBoots), 1);
    }

    #[test]
    fn trail_boots_contribute_traversal_bonus() {
        let mut equipment = Equipment::default();
        equipment.equip(ItemKind::TrailBoots);
        assert!((equipment.movement_bonus() - 0.08).abs() < f32::EPSILON);
    }
}
