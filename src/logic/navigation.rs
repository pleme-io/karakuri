//! Pure navigation logic — no ECS, no macOS APIs.
//!
//! Extracted from `commands.rs` so that directional window and display
//! lookups can be unit-tested without Bevy or accessibility dependencies.

use bevy::ecs::entity::Entity;
use bevy::math::IRect;

use crate::commands::Direction;
use crate::manager::{Column, LayoutStrip};

/// Retrieves the window entity in a specified direction relative to a current
/// entity within a `LayoutStrip`.
///
/// For East/West: navigates across columns. For North/South: navigates within
/// a stack. For First/Last: returns the first/last column's top entity.
pub fn window_in_direction(
    direction: &Direction,
    entity: Entity,
    strip: &LayoutStrip,
) -> Option<Entity> {
    let index = strip.index_of(entity).ok()?;

    match direction {
        Direction::West => strip.left_neighbour(entity),
        Direction::East => strip.right_neighbour(entity),

        Direction::First => strip.first().ok().and_then(|column| column.top()),
        Direction::Last => strip.last().ok().and_then(|column| column.top()),

        Direction::North => match strip.get(index).ok()? {
            Column::Single(_) => None,
            Column::Stack(stack) => stack
                .iter()
                .enumerate()
                .find(|(_, window_id)| entity == **window_id)
                .and_then(|(index, _)| (index > 0).then(|| stack.get(index - 1)).flatten())
                .copied(),
        },

        Direction::South => match strip.get(index).ok()? {
            Column::Single(_) => None,
            Column::Stack(stack) => stack
                .iter()
                .enumerate()
                .find(|(_, window_id)| entity == **window_id)
                .and_then(|(index, _)| {
                    (index < stack.len() - 1)
                        .then(|| stack.get(index + 1))
                        .flatten()
                })
                .copied(),
        },
    }
}

/// Finds the nearest display in the given direction from `active_bounds`.
///
/// For West/East: picks the display whose horizontal edge is closest.
/// For North/South: picks the display whose vertical edge is closest.
/// Returns `None` if no display exists in that direction.
pub fn display_in_direction(
    direction: &Direction,
    active_bounds: IRect,
    others: &[IRect],
) -> Option<usize> {
    others
        .iter()
        .enumerate()
        .filter(|(_, b)| match direction {
            Direction::West => b.min.x < active_bounds.min.x,
            Direction::East => b.min.x >= active_bounds.max.x,
            Direction::North => b.min.y < active_bounds.min.y,
            Direction::South => b.min.y >= active_bounds.max.y,
            Direction::First | Direction::Last => false,
        })
        .min_by_key(|(_, b)| match direction {
            Direction::West => active_bounds.min.x - b.max.x,
            Direction::East => b.min.x - active_bounds.max.x,
            Direction::North => active_bounds.min.y - b.max.y,
            Direction::South => b.min.y - active_bounds.max.y,
            Direction::First | Direction::Last => i32::MAX,
        })
        .map(|(idx, _)| idx)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::prelude::*;

    // ── window_in_direction ────────────────────────────────────────────

    fn setup_strip() -> (World, LayoutStrip, Vec<Entity>) {
        let mut world = World::new();
        let entities = world
            .spawn_batch(vec![(), (), (), ()])
            .collect::<Vec<Entity>>();

        let mut strip = LayoutStrip::default();
        strip.append(entities[0]);
        strip.append(entities[1]);
        strip.append(entities[2]);
        strip.append(entities[3]);
        strip.stack(entities[1]).unwrap(); // Stack e1 onto e0

        (world, strip, entities)
    }

    #[test]
    fn east_from_middle_returns_next() {
        let (_world, strip, e) = setup_strip();
        assert_eq!(
            window_in_direction(&Direction::East, e[2], &strip),
            Some(e[3])
        );
    }

    #[test]
    fn west_from_middle_returns_previous() {
        let (_world, strip, e) = setup_strip();
        assert_eq!(
            window_in_direction(&Direction::West, e[2], &strip),
            Some(e[0])
        );
    }

    #[test]
    fn east_from_last_returns_none() {
        let (_world, strip, e) = setup_strip();
        assert_eq!(window_in_direction(&Direction::East, e[3], &strip), None);
    }

    #[test]
    fn west_from_first_returns_none() {
        let (_world, strip, e) = setup_strip();
        assert_eq!(window_in_direction(&Direction::West, e[0], &strip), None);
    }

    #[test]
    fn south_in_stack_returns_below() {
        let (_world, strip, e) = setup_strip();
        assert_eq!(
            window_in_direction(&Direction::South, e[0], &strip),
            Some(e[1])
        );
    }

    #[test]
    fn north_in_stack_returns_above() {
        let (_world, strip, e) = setup_strip();
        assert_eq!(
            window_in_direction(&Direction::North, e[1], &strip),
            Some(e[0])
        );
    }

    #[test]
    fn north_at_top_of_stack_returns_none() {
        let (_world, strip, e) = setup_strip();
        assert_eq!(window_in_direction(&Direction::North, e[0], &strip), None);
    }

    #[test]
    fn south_at_bottom_of_stack_returns_none() {
        let (_world, strip, e) = setup_strip();
        assert_eq!(window_in_direction(&Direction::South, e[1], &strip), None);
    }

    #[test]
    fn south_on_single_returns_none() {
        let (_world, strip, e) = setup_strip();
        assert_eq!(window_in_direction(&Direction::South, e[2], &strip), None);
    }

    #[test]
    fn north_on_single_returns_none() {
        let (_world, strip, e) = setup_strip();
        assert_eq!(window_in_direction(&Direction::North, e[2], &strip), None);
    }

    #[test]
    fn first_returns_first_column_top() {
        let (_world, strip, e) = setup_strip();
        assert_eq!(
            window_in_direction(&Direction::First, e[3], &strip),
            Some(e[0])
        );
    }

    #[test]
    fn last_returns_last_column_top() {
        let (_world, strip, e) = setup_strip();
        assert_eq!(
            window_in_direction(&Direction::Last, e[0], &strip),
            Some(e[3])
        );
    }

    #[test]
    fn adjacent_stacks_east_west() {
        let mut world = World::new();
        let e = world
            .spawn_batch(vec![(), (), (), ()])
            .collect::<Vec<Entity>>();

        let mut strip = LayoutStrip::default();
        strip.append(e[0]);
        strip.append(e[1]);
        strip.append(e[2]);
        strip.append(e[3]);
        strip.stack(e[1]).unwrap(); // [Stack(e0,e1), e2, e3]
        strip.stack(e[3]).unwrap(); // [Stack(e0,e1), Stack(e2,e3)]

        // East across stacks
        assert_eq!(
            window_in_direction(&Direction::East, e[0], &strip),
            Some(e[2])
        );
        assert_eq!(
            window_in_direction(&Direction::East, e[1], &strip),
            Some(e[3])
        );
        // West across stacks
        assert_eq!(
            window_in_direction(&Direction::West, e[2], &strip),
            Some(e[0])
        );
        assert_eq!(
            window_in_direction(&Direction::West, e[3], &strip),
            Some(e[1])
        );
    }

    #[test]
    fn empty_strip_returns_none() {
        let mut world = World::new();
        let e = world.spawn_batch(vec![()]).collect::<Vec<Entity>>();
        let strip = LayoutStrip::default();
        assert_eq!(window_in_direction(&Direction::East, e[0], &strip), None);
        assert_eq!(window_in_direction(&Direction::First, e[0], &strip), None);
    }

    #[test]
    fn single_window_strip() {
        let mut world = World::new();
        let e = world.spawn_batch(vec![()]).collect::<Vec<Entity>>();
        let mut strip = LayoutStrip::default();
        strip.append(e[0]);

        assert_eq!(window_in_direction(&Direction::East, e[0], &strip), None);
        assert_eq!(window_in_direction(&Direction::West, e[0], &strip), None);
        assert_eq!(
            window_in_direction(&Direction::First, e[0], &strip),
            Some(e[0])
        );
        assert_eq!(
            window_in_direction(&Direction::Last, e[0], &strip),
            Some(e[0])
        );
    }

    // ── display_in_direction ───────────────────────────────────────────

    #[test]
    fn display_east_finds_nearest() {
        let active = IRect::new(0, 0, 1920, 1080);
        let others = vec![
            IRect::new(1920, 0, 3840, 1080), // immediately right
            IRect::new(3840, 0, 5760, 1080), // further right
        ];
        assert_eq!(display_in_direction(&Direction::East, active, &others), Some(0));
    }

    #[test]
    fn display_west_finds_nearest() {
        let active = IRect::new(1920, 0, 3840, 1080);
        let others = vec![
            IRect::new(0, 0, 1920, 1080), // immediately left
        ];
        assert_eq!(display_in_direction(&Direction::West, active, &others), Some(0));
    }

    #[test]
    fn display_no_match_returns_none() {
        let active = IRect::new(0, 0, 1920, 1080);
        let others = vec![
            IRect::new(1920, 0, 3840, 1080), // only to the right
        ];
        assert_eq!(display_in_direction(&Direction::West, active, &others), None);
    }

    #[test]
    fn display_south_finds_below() {
        let active = IRect::new(0, 0, 1920, 1080);
        let others = vec![
            IRect::new(0, 1080, 1920, 2160), // directly below
        ];
        assert_eq!(
            display_in_direction(&Direction::South, active, &others),
            Some(0)
        );
    }

    #[test]
    fn display_north_finds_above() {
        let active = IRect::new(0, 1080, 1920, 2160);
        let others = vec![
            IRect::new(0, 0, 1920, 1080), // directly above
        ];
        assert_eq!(
            display_in_direction(&Direction::North, active, &others),
            Some(0)
        );
    }

    #[test]
    fn display_empty_others() {
        let active = IRect::new(0, 0, 1920, 1080);
        let others: Vec<IRect> = vec![];
        assert_eq!(display_in_direction(&Direction::East, active, &others), None);
    }

    #[test]
    fn display_first_last_always_none() {
        let active = IRect::new(0, 0, 1920, 1080);
        let others = vec![IRect::new(1920, 0, 3840, 1080)];
        assert_eq!(
            display_in_direction(&Direction::First, active, &others),
            None
        );
        assert_eq!(
            display_in_direction(&Direction::Last, active, &others),
            None
        );
    }

    #[test]
    fn display_multiple_east_picks_closest() {
        let active = IRect::new(0, 0, 1920, 1080);
        let others = vec![
            IRect::new(3840, 0, 5760, 1080), // further right (idx 0)
            IRect::new(1920, 0, 3840, 1080), // immediately right (idx 1)
        ];
        assert_eq!(
            display_in_direction(&Direction::East, active, &others),
            Some(1)
        );
    }

    #[test]
    fn display_diagonal_below_right() {
        let active = IRect::new(0, 0, 1920, 1080);
        let others = vec![
            IRect::new(1920, 1080, 3840, 2160), // diagonal: right and below
        ];
        // East: min.x (1920) >= max.x (1920) → true → found
        assert_eq!(
            display_in_direction(&Direction::East, active, &others),
            Some(0)
        );
        // South: min.y (1080) >= max.y (1080) → true → found
        assert_eq!(
            display_in_direction(&Direction::South, active, &others),
            Some(0)
        );
    }

    #[test]
    fn display_gap_between_monitors() {
        let active = IRect::new(0, 0, 1920, 1080);
        let others = vec![
            IRect::new(2000, 0, 3920, 1080), // 80px gap to the right
        ];
        assert_eq!(
            display_in_direction(&Direction::East, active, &others),
            Some(0)
        );
    }

    #[test]
    fn window_first_last_same_on_single() {
        let mut world = World::new();
        let e = world.spawn_batch(vec![()]).collect::<Vec<Entity>>();
        let mut strip = LayoutStrip::default();
        strip.append(e[0]);

        let first = window_in_direction(&Direction::First, e[0], &strip);
        let last = window_in_direction(&Direction::Last, e[0], &strip);
        assert_eq!(first, last);
        assert_eq!(first, Some(e[0]));
    }

    #[test]
    fn window_deep_stack_traversal() {
        let mut world = World::new();
        let e = world
            .spawn_batch(vec![(), (), (), (), ()])
            .collect::<Vec<Entity>>();
        let mut strip = LayoutStrip::default();
        strip.append(e[0]);
        strip.append(e[1]);
        strip.append(e[2]);
        strip.append(e[3]);
        strip.append(e[4]);
        strip.stack(e[1]).unwrap(); // Stack e1 onto e0
        strip.stack(e[2]).unwrap(); // Stack e2 onto e0,e1
        // Strip: [Stack(e0,e1,e2), e3, e4]

        assert_eq!(
            window_in_direction(&Direction::South, e[0], &strip),
            Some(e[1])
        );
        assert_eq!(
            window_in_direction(&Direction::South, e[1], &strip),
            Some(e[2])
        );
        assert_eq!(
            window_in_direction(&Direction::South, e[2], &strip),
            None
        );
        assert_eq!(
            window_in_direction(&Direction::North, e[2], &strip),
            Some(e[1])
        );
    }
}
