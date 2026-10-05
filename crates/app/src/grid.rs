//! Justified rows, the Flickr / Google Photos layout: every tile in a row has
//! the same height, each keeps its own shape, and the row is scaled so its
//! edges line up with the window on both sides.

#[derive(Debug, Clone, PartialEq)]
pub struct Tile {
    pub item: usize,
    pub x: f32,
    pub width: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub height: f32,
    pub tiles: Vec<Tile>,
}

pub fn justify(aspects: &[f32], width: f32, target: f32, gap: f32) -> Vec<Row> {
    let mut rows = Vec::new();
    let mut start = 0;

    while start < aspects.len() {
        let mut end = start;
        let mut sum = 0.0;
        // Grow the row until it's too wide at the target height, then keep
        // whichever of "with the last one" or "without it" lands closer.
        loop {
            sum += aspects[end];
            end += 1;
            let gaps = gap * (end - start - 1) as f32;
            if sum * target + gaps >= width || end == aspects.len() {
                break;
            }
        }

        let height_with = fit(sum, width, gap, end - start);
        let last_row =
            end == aspects.len() && sum * target + gap * (end - start - 1) as f32 <= width;
        let height = if last_row {
            // A short last row stays at the target height, stretching it to
            // the full width would blow two screenshots up to poster size.
            target
        } else if end - start > 1 {
            let without = sum - aspects[end - 1];
            let height_without = fit(without, width, gap, end - start - 1);
            if (height_without / target - 1.0).abs() < (1.0 - height_with / target).abs() {
                end -= 1;
                height_without
            } else {
                height_with
            }
        } else {
            height_with
        };

        let mut x = 0.0;
        let tiles = (start..end)
            .map(|item| {
                let w = aspects[item] * height;
                let tile = Tile { item, x, width: w };
                x += w + gap;
                tile
            })
            .collect();
        rows.push(Row { height, tiles });
        start = end;
    }
    rows
}

fn fit(aspect_sum: f32, width: f32, gap: f32, count: usize) -> f32 {
    (width - gap * (count as f32 - 1.0)) / aspect_sum
}

/// For every item, which row it's in and where in that row.
pub fn locate(rows: &[Row], items: usize) -> Vec<(usize, usize)> {
    let mut at = vec![(0, 0); items];
    for (r, row) in rows.iter().enumerate() {
        for (c, tile) in row.tiles.iter().enumerate() {
            at[tile.item] = (r, c);
        }
    }
    at
}

/// The item straight above or below, meaning the one whose centre is closest
/// horizontally, not the one at the same position in the row.
pub fn vertical(rows: &[Row], from: (usize, usize), down: bool) -> Option<usize> {
    let (r, c) = from;
    let target = if down { r + 1 } else { r.checked_sub(1)? };
    let row = rows.get(target)?;
    let tile = &rows[r].tiles[c];
    let centre = tile.x + tile.width / 2.0;
    row.tiles
        .iter()
        .min_by(|a, b| {
            let da = (a.x + a.width / 2.0 - centre).abs();
            let db = (b.x + b.width / 2.0 - centre).abs();
            da.total_cmp(&db)
        })
        .map(|t| t.item)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn right_edge(row: &Row) -> f32 {
        let last = row.tiles.last().unwrap();
        last.x + last.width
    }

    #[test]
    fn full_rows_fill_the_width_exactly() {
        let aspects = [1.78, 1.5, 0.6, 2.4, 1.0, 1.78, 1.33, 0.5, 3.0, 1.78];
        let rows = justify(&aspects, 1000.0, 180.0, 8.0);
        for row in &rows[..rows.len() - 1] {
            assert!(
                (right_edge(row) - 1000.0).abs() < 0.01,
                "{}",
                right_edge(row)
            );
        }
    }

    #[test]
    fn every_item_lands_once_and_in_order() {
        let aspects = [1.78; 23];
        let rows = justify(&aspects, 1180.0, 170.0, 6.0);
        let items: Vec<usize> = rows
            .iter()
            .flat_map(|r| r.tiles.iter().map(|t| t.item))
            .collect();
        assert_eq!(items, (0..23).collect::<Vec<_>>());
    }

    #[test]
    fn heights_stay_near_the_target() {
        let aspects = [
            1.78, 1.5, 0.6, 2.4, 1.0, 1.78, 1.33, 0.5, 3.0, 1.78, 1.2, 0.9,
        ];
        for row in justify(&aspects, 1000.0, 180.0, 8.0) {
            assert!(row.height > 120.0 && row.height < 260.0, "{}", row.height);
        }
    }

    #[test]
    fn a_short_last_row_is_not_stretched() {
        let rows = justify(&[1.78, 1.78, 1.78, 1.78, 1.0], 1000.0, 180.0, 8.0);
        let last = rows.last().unwrap();
        assert_eq!(last.height, 180.0);
        assert!(right_edge(last) < 1000.0);
    }

    #[test]
    fn one_item_and_no_items() {
        assert!(justify(&[], 800.0, 180.0, 8.0).is_empty());
        let rows = justify(&[1.5], 800.0, 180.0, 8.0);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].height, 180.0);
    }

    #[test]
    fn a_window_narrower_than_one_tile_still_lays_out() {
        let rows = justify(&[3.0, 3.0], 200.0, 180.0, 8.0);
        assert_eq!(rows.len(), 2);
        assert!((right_edge(&rows[0]) - 200.0).abs() < 0.01);
    }

    #[test]
    fn up_and_down_pick_the_nearest_centre() {
        // row 0: two wide tiles, row 1: four narrow ones
        let rows = vec![
            Row {
                height: 100.0,
                tiles: vec![
                    Tile {
                        item: 0,
                        x: 0.0,
                        width: 500.0,
                    },
                    Tile {
                        item: 1,
                        x: 500.0,
                        width: 500.0,
                    },
                ],
            },
            Row {
                height: 100.0,
                tiles: (0..4)
                    .map(|i| Tile {
                        item: 2 + i,
                        x: i as f32 * 250.0,
                        width: 250.0,
                    })
                    .collect(),
            },
        ];
        assert_eq!(vertical(&rows, (0, 1), true), Some(4));
        assert_eq!(vertical(&rows, (1, 3), false), Some(1));
        assert_eq!(vertical(&rows, (0, 0), false), None);
        assert_eq!(vertical(&rows, (1, 0), true), None);
        assert_eq!(locate(&rows, 6)[5], (1, 3));
    }
}
