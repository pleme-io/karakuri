/// Pure bar layout math — no platform or Bevy dependencies.
///
/// Given a list of items with their measured sizes and positions,
/// computes the final x-coordinate for each item on the bar.

/// A measured bar item ready for layout.
#[derive(Clone, Debug)]
pub struct BarItemMeasure {
    /// Item index (used to map back to the entity).
    pub index: usize,
    /// Which position group this item belongs to.
    pub position: LayoutPosition,
    /// Sort order within the position group.
    pub order: u32,
    /// Measured width of the item (icon + label + padding).
    pub width: f64,
    /// Left padding.
    pub padding_left: f64,
    /// Right padding.
    pub padding_right: f64,
}

/// Simplified position enum for layout (avoids serde dependency in logic).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LayoutPosition {
    Left,
    Center,
    Right,
    /// Left of notch.
    Q,
    /// Right of notch.
    E,
}

/// Result of layout computation for a single item.
#[derive(Clone, Debug)]
pub struct BarItemPlacement {
    /// Item index (maps back to entity).
    pub index: usize,
    /// Final x-coordinate in bar-local coords.
    pub x: f64,
}

/// Compute final x positions for all items on a bar.
///
/// Items are sorted by order within each position group, then stacked:
/// - `Left`: stacks left-to-right from `bar_padding_left`
/// - `Right`: stacks right-to-left from `bar_width - bar_padding_right`
/// - `Center`: centered on `bar_width / 2`
/// - `Q`: stacks left-to-right from `notch_left - items_width` (left of notch)
/// - `E`: stacks left-to-right from `notch_right` (right of notch)
pub fn compute_bar_layout(
    items: &[BarItemMeasure],
    bar_width: f64,
    bar_padding_left: f64,
    bar_padding_right: f64,
    notch_center_x: f64,
    notch_width: f64,
) -> Vec<BarItemPlacement> {
    let mut placements = Vec::with_capacity(items.len());

    let mut left_items: Vec<&BarItemMeasure> = items
        .iter()
        .filter(|i| i.position == LayoutPosition::Left)
        .collect();
    left_items.sort_by_key(|i| i.order);

    let mut right_items: Vec<&BarItemMeasure> = items
        .iter()
        .filter(|i| i.position == LayoutPosition::Right)
        .collect();
    right_items.sort_by_key(|i| i.order);

    let mut center_items: Vec<&BarItemMeasure> = items
        .iter()
        .filter(|i| i.position == LayoutPosition::Center)
        .collect();
    center_items.sort_by_key(|i| i.order);

    let mut q_items: Vec<&BarItemMeasure> = items
        .iter()
        .filter(|i| i.position == LayoutPosition::Q)
        .collect();
    q_items.sort_by_key(|i| i.order);

    let mut e_items: Vec<&BarItemMeasure> = items
        .iter()
        .filter(|i| i.position == LayoutPosition::E)
        .collect();
    e_items.sort_by_key(|i| i.order);

    // Left: stack from left edge
    let mut x = bar_padding_left;
    for item in &left_items {
        let total_w = item.padding_left + item.width + item.padding_right;
        placements.push(BarItemPlacement {
            index: item.index,
            x: x + item.padding_left,
        });
        x += total_w;
    }

    // Right: stack from right edge (right-to-left, then flip)
    let mut x = bar_width - bar_padding_right;
    for item in &right_items {
        let total_w = item.padding_left + item.width + item.padding_right;
        x -= total_w;
        placements.push(BarItemPlacement {
            index: item.index,
            x: x + item.padding_left,
        });
    }

    // Center: compute total width, then center on bar midpoint
    let center_total_w: f64 = center_items
        .iter()
        .map(|i| i.padding_left + i.width + i.padding_right)
        .sum();
    let mut x = (bar_width - center_total_w) / 2.0;
    for item in &center_items {
        placements.push(BarItemPlacement {
            index: item.index,
            x: x + item.padding_left,
        });
        x += item.padding_left + item.width + item.padding_right;
    }

    // Q: stack right-to-left ending at left edge of notch
    let notch_left = notch_center_x - notch_width / 2.0;
    let q_total_w: f64 = q_items
        .iter()
        .map(|i| i.padding_left + i.width + i.padding_right)
        .sum();
    let mut x = notch_left - q_total_w;
    for item in &q_items {
        placements.push(BarItemPlacement {
            index: item.index,
            x: x + item.padding_left,
        });
        x += item.padding_left + item.width + item.padding_right;
    }

    // E: stack left-to-right from right edge of notch
    let notch_right = notch_center_x + notch_width / 2.0;
    let mut x = notch_right;
    for item in &e_items {
        placements.push(BarItemPlacement {
            index: item.index,
            x: x + item.padding_left,
        });
        x += item.padding_left + item.width + item.padding_right;
    }

    placements
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_item(index: usize, position: LayoutPosition, order: u32, width: f64) -> BarItemMeasure {
        BarItemMeasure {
            index,
            position,
            order,
            width,
            padding_left: 4.0,
            padding_right: 4.0,
        }
    }

    #[test]
    fn left_items_stack_from_edge() {
        let items = vec![
            make_item(0, LayoutPosition::Left, 0, 50.0),
            make_item(1, LayoutPosition::Left, 1, 30.0),
        ];
        let placements = compute_bar_layout(&items, 1000.0, 8.0, 8.0, 500.0, 0.0);
        let p0 = placements.iter().find(|p| p.index == 0).unwrap();
        let p1 = placements.iter().find(|p| p.index == 1).unwrap();
        // First item: bar_padding(8) + item_padding(4) = 12
        assert!((p0.x - 12.0).abs() < 0.01);
        // Second item: 8 + (4+50+4) + 4 = 70
        assert!((p1.x - 70.0).abs() < 0.01);
    }

    #[test]
    fn right_items_stack_from_edge() {
        let items = vec![
            make_item(0, LayoutPosition::Right, 0, 50.0),
            make_item(1, LayoutPosition::Right, 1, 30.0),
        ];
        let placements = compute_bar_layout(&items, 1000.0, 8.0, 8.0, 500.0, 0.0);
        let p0 = placements.iter().find(|p| p.index == 0).unwrap();
        let p1 = placements.iter().find(|p| p.index == 1).unwrap();
        // First right item (order 0): 1000 - 8 - (4+50+4) + 4 = 938
        assert!((p0.x - 938.0).abs() < 0.01);
        // Second right item (order 1): 938 - 4 - (4+30+4) + 4 = 900
        assert!((p1.x - 900.0).abs() < 0.01);
    }

    #[test]
    fn center_items_centered() {
        let items = vec![make_item(0, LayoutPosition::Center, 0, 100.0)];
        let placements = compute_bar_layout(&items, 1000.0, 8.0, 8.0, 500.0, 0.0);
        let p0 = placements.iter().find(|p| p.index == 0).unwrap();
        // Total width: 4+100+4 = 108. Center: (1000-108)/2 + 4 = 450
        assert!((p0.x - 450.0).abs() < 0.01);
    }

    #[test]
    fn items_sorted_by_order() {
        let items = vec![
            make_item(0, LayoutPosition::Left, 2, 40.0),
            make_item(1, LayoutPosition::Left, 0, 40.0),
            make_item(2, LayoutPosition::Left, 1, 40.0),
        ];
        let placements = compute_bar_layout(&items, 1000.0, 8.0, 8.0, 500.0, 0.0);
        // Order 0 (index 1) should be first
        let p1 = placements.iter().find(|p| p.index == 1).unwrap();
        let p2 = placements.iter().find(|p| p.index == 2).unwrap();
        let p0 = placements.iter().find(|p| p.index == 0).unwrap();
        assert!(p1.x < p2.x);
        assert!(p2.x < p0.x);
    }

    #[test]
    fn empty_items_returns_empty() {
        let placements = compute_bar_layout(&[], 1000.0, 8.0, 8.0, 500.0, 0.0);
        assert!(placements.is_empty());
    }

    #[test]
    fn notch_positions() {
        let items = vec![
            make_item(0, LayoutPosition::Q, 0, 60.0),
            make_item(1, LayoutPosition::E, 0, 60.0),
        ];
        let placements = compute_bar_layout(&items, 1000.0, 8.0, 8.0, 500.0, 200.0);
        let pq = placements.iter().find(|p| p.index == 0).unwrap();
        let pe = placements.iter().find(|p| p.index == 1).unwrap();
        // Q: notch_left = 500-100 = 400. Q total = 68. Start = 400-68 = 332. x = 332+4 = 336
        assert!((pq.x - 336.0).abs() < 0.01);
        // E: notch_right = 500+100 = 600. x = 600+4 = 604
        assert!((pe.x - 604.0).abs() < 0.01);
    }

    #[test]
    fn mixed_positions() {
        let items = vec![
            make_item(0, LayoutPosition::Left, 0, 50.0),
            make_item(1, LayoutPosition::Right, 0, 50.0),
            make_item(2, LayoutPosition::Center, 0, 80.0),
        ];
        let placements = compute_bar_layout(&items, 1000.0, 8.0, 8.0, 500.0, 0.0);
        assert_eq!(placements.len(), 3);
        // All three should have valid positions
        for p in &placements {
            assert!(p.x >= 0.0);
            assert!(p.x < 1000.0);
        }
    }
}
