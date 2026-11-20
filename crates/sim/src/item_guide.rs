//! Item relationships for the book. The caller applies its world's discovery rules before showing them.

use crate::caves::{CAVE_LEVELS, ore_item, vein_distribution};
use crate::rules::{BuildingDef, RecipeDef, Rules};
use crate::{Discovery, ItemKind, NodeKind};

/// Recipes that can produce an item. A mining job yields the ore at the shaft's deposit.
pub fn made_by(rules: &Rules, item: ItemKind) -> impl Iterator<Item = (usize, &RecipeDef)> {
    rules.recipes.iter().enumerate().filter(move |(_, recipe)| {
        recipe.discovery != Discovery::Retired
            && (recipe.outputs.iter().any(|output| output.item == item)
                || recipe
                    .alloy
                    .is_some_and(|alloy| item == alloy.output || item == alloy.bad)
                || recipe.mine
                    && matches!(
                        item,
                        ItemKind::IronOre
                            | ItemKind::CopperOre
                            | ItemKind::TinOre
                            | ItemKind::Coal
                            | ItemKind::Oil
                            | ItemKind::Bauxite
                    ))
    })
}

/// Recipes that consume or require an item, including ores loaded separately for alloying.
pub fn used_in(rules: &Rules, item: ItemKind) -> impl Iterator<Item = (usize, &RecipeDef)> {
    rules.recipes.iter().enumerate().filter(move |(_, recipe)| {
        recipe.discovery != Discovery::Retired
            && (recipe.inputs.iter().any(|input| input.item == item)
                || recipe.needs.contains(&item)
                || recipe
                    .alloy
                    .is_some_and(|alloy| item == alloy.base || item == alloy.add))
    })
}

/// Buildings that can consume or require an item when built.
pub fn used_for_building(rules: &Rules, item: ItemKind) -> impl Iterator<Item = &BuildingDef> {
    rules.buildings.iter().filter(move |building| {
        building.discovery != Discovery::Retired
            && (building.needs.contains(&item)
                || building
                    .price_lists()
                    .flatten()
                    .any(|cost| cost.item == item))
    })
}

/// Resource nodes that can lead to an item. A cave mouth describes mining inside the cave.
pub fn found_in(item: ItemKind) -> impl Iterator<Item = NodeKind> {
    use ItemKind as I;
    use NodeKind as N;
    let nodes = [
        N::StoneOutcrop,
        N::Tree,
        N::BerryBush,
        N::ClayDeposit,
        N::IronOutcrop,
        N::HerbPatch,
        N::CopperOutcrop,
        N::TinOutcrop,
        N::Deadwood,
        N::FiberPlant,
        N::CaveMouth,
    ];
    nodes.into_iter().filter(move |node| {
        node.item() == item
            || match node {
                N::StoneOutcrop => matches!(item, I::Flint | I::Limestone),
                N::Tree => item == I::Branch,
                N::ClayDeposit => matches!(item, I::FireClay | I::Sand),
                N::FiberPlant => {
                    matches!(item, I::Seed | I::Lentil | I::Flax | I::Linseed | I::Cotton)
                }
                N::CaveMouth => cave_depths(item).next().is_some(),
                _ => false,
            }
    })
}

/// Zero-based cave depth bands in which a vein can yield the item.
pub fn cave_depths(item: ItemKind) -> impl Iterator<Item = u8> {
    (0..CAVE_LEVELS).filter(move |&depth| {
        vein_distribution(depth)
            .iter()
            .any(|&(code, _)| ore_item(code) == Some(item))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BuildingKind, rules};

    #[test]
    fn recipes_include_station_outputs_and_special_materials() {
        let defs = rules();
        assert!(made_by(defs, ItemKind::StoneAxe).any(|(_, r)| r.id == "stone_axe"));
        assert!(made_by(defs, ItemKind::Bronze).any(|(_, r)| r.id == "alloy_bronze"));
        assert!(used_in(defs, ItemKind::TinOre).any(|(_, r)| r.id == "alloy_bronze"));
        assert!(used_in(defs, ItemKind::Flint).any(|(_, r)| r.needs.contains(&ItemKind::Flint)));
        assert!(made_by(defs, ItemKind::Bauxite).any(|(_, r)| r.mine));
        assert!(!made_by(defs, ItemKind::Fiber).any(|(_, r)| r.mine));
    }

    #[test]
    fn buildings_include_alternate_costs_and_reusable_items() {
        let defs = rules();
        assert!(used_for_building(defs, ItemKind::Flint).any(|b| b.kind == BuildingKind::Campfire));
        assert!(
            used_for_building(defs, ItemKind::WindowPane).any(|b| b.kind == BuildingKind::Shelter)
        );
        assert!(used_for_building(defs, ItemKind::Chip).any(|b| b.kind == BuildingKind::Computer));
    }

    #[test]
    fn sources_cover_both_primary_and_secondary_yields() {
        assert_eq!(cave_depths(ItemKind::Coal).collect::<Vec<_>>(), [2, 3]);
        assert_eq!(
            found_in(ItemKind::CopperOre).collect::<Vec<_>>(),
            [NodeKind::CopperOutcrop, NodeKind::CaveMouth]
        );
        assert_eq!(
            found_in(ItemKind::Flint).collect::<Vec<_>>(),
            [NodeKind::StoneOutcrop]
        );
        assert_eq!(
            found_in(ItemKind::FireClay).collect::<Vec<_>>(),
            [NodeKind::ClayDeposit]
        );
        assert_eq!(
            found_in(ItemKind::Linseed).collect::<Vec<_>>(),
            [NodeKind::FiberPlant]
        );
        assert!(found_in(ItemKind::StoneAxe).next().is_none());
    }
}
