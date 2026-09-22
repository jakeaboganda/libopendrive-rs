//! A uniform grid over the XY ground plane, for nearest-thing-under-a-point
//! queries.
//!
//! Both hot lookups on a city map are the same shape: given `(x, y)`, find the
//! nearest of thousands of things that each occupy a small patch of ground.
//! Scanning all of them is O(map) per query, and a consumer runs these every
//! tick for every body, so the cost scales with map size times fleet size.
//!
//! The grid buckets items by their XY bounding box, then answers a query by
//! walking cells outward in rings and stopping as soon as the best candidate
//! found is closer than anything the next ring could hold. Items are bucketed
//! by bounding box, which is a superset of their geometry, so nothing that
//! could win is ever skipped.

/// An axis-aligned XY footprint.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Aabb {
    pub min_x: f32,
    pub max_x: f32,
    pub min_y: f32,
    pub max_y: f32,
}

impl Aabb {
    /// The box around a set of XY points, or `None` if there are none.
    pub fn around(points: impl IntoIterator<Item = (f32, f32)>) -> Option<Self> {
        points.into_iter().fold(None, |acc: Option<Self>, (x, y)| {
            Some(match acc {
                None => Self {
                    min_x: x,
                    max_x: x,
                    min_y: y,
                    max_y: y,
                },
                Some(b) => Self {
                    min_x: b.min_x.min(x),
                    max_x: b.max_x.max(x),
                    min_y: b.min_y.min(y),
                    max_y: b.max_y.max(y),
                },
            })
        })
    }

    fn merge(self, other: Self) -> Self {
        Self {
            min_x: self.min_x.min(other.min_x),
            max_x: self.max_x.max(other.max_x),
            min_y: self.min_y.min(other.min_y),
            max_y: self.max_y.max(other.max_y),
        }
    }

    /// Squared distance from `(x, y)` to this box; zero inside it. A lower
    /// bound on the distance to whatever the box contains, so a candidate
    /// whose box is already further than the best hit can be rejected without
    /// touching its geometry.
    pub fn dist2(&self, x: f32, y: f32) -> f32 {
        let dx = (self.min_x - x).max(0.0).max(x - self.max_x);
        let dy = (self.min_y - y).max(0.0).max(y - self.max_y);
        dx * dx + dy * dy
    }
}

/// Item indices bucketed into square cells.
#[derive(Debug, Clone, Default)]
pub(crate) struct Grid {
    min_x: f32,
    min_y: f32,
    cell: f32,
    cols: usize,
    rows: usize,
    /// Row-major `rows * cols` buckets of indices into the caller's item list.
    cells: Vec<Vec<u32>>,
}

/// Cap on a grid side, so a map with one enormous item can't allocate a
/// pathological number of cells.
const MAX_DIM: usize = 1024;
/// Items per cell to aim for. Low enough that a ring holds few candidates,
/// high enough that the ring walk isn't all bookkeeping.
const ITEMS_PER_CELL: f32 = 2.0;

impl Grid {
    /// Bucket `bounds` (one box per item, indexed as given). An empty list
    /// yields an empty grid, which answers every query with `None`.
    pub fn build(bounds: &[Aabb]) -> Self {
        let Some(extent) = bounds.iter().copied().reduce(Aabb::merge) else {
            return Self::default();
        };
        let (w, h) = (extent.max_x - extent.min_x, extent.max_y - extent.min_y);
        let target = (bounds.len() as f32 / ITEMS_PER_CELL).max(1.0);
        // Area per cell, floored so a perfectly straight road (zero extent on
        // one axis) still produces a usable cell size.
        let area = w.max(1e-3) * h.max(1e-3);
        let cell = (area / target)
            .sqrt()
            .max(w.max(h) / MAX_DIM as f32)
            .max(1e-3);
        let cols = ((w / cell).ceil() as usize).clamp(1, MAX_DIM);
        let rows = ((h / cell).ceil() as usize).clamp(1, MAX_DIM);
        // Re-derive the cell size from the clamped dimensions, so the grid is
        // guaranteed to span the whole extent.
        let cell = (w / cols as f32).max(h / rows as f32).max(1e-3);

        let mut grid = Self {
            min_x: extent.min_x,
            min_y: extent.min_y,
            cell,
            cols,
            rows,
            cells: vec![Vec::new(); cols * rows],
        };
        for (i, b) in bounds.iter().enumerate() {
            let (lo_x, hi_x) = (grid.col(b.min_x), grid.col(b.max_x));
            let (lo_y, hi_y) = (grid.row(b.min_y), grid.row(b.max_y));
            for iy in lo_y..=hi_y {
                for ix in lo_x..=hi_x {
                    grid.cells[iy * cols + ix].push(i as u32);
                }
            }
        }
        grid
    }

    fn col(&self, x: f32) -> usize {
        Self::bucket(x - self.min_x, self.cell, self.cols)
    }

    fn row(&self, y: f32) -> usize {
        Self::bucket(y - self.min_y, self.cell, self.rows)
    }

    fn bucket(offset: f32, cell: f32, count: usize) -> usize {
        // A NaN query coordinate lands in cell 0 rather than panicking: it can
        // only fail to find anything, which is what a NaN deserves.
        let i = (offset / cell).floor();
        if i.is_nan() || i < 0.0 {
            0
        } else {
            (i as usize).min(count - 1)
        }
    }

    /// The items whose footprint could contain `(x, y)`. Everything that does
    /// contain it is here; some of what is here does not. Empty when the point
    /// falls outside the grid's extent, or the grid holds nothing.
    pub fn at(&self, x: f32, y: f32) -> &[u32] {
        if self.cells.is_empty() || !self.covers(x, y) {
            return &[];
        }
        &self.cells[self.row(y) * self.cols + self.col(x)]
    }

    /// Whether `(x, y)` is inside the grid's extent. Outside it, `col`/`row`
    /// clamp to an edge cell, which is right for a nearest-item walk and wrong
    /// for a containment lookup.
    fn covers(&self, x: f32, y: f32) -> bool {
        let (dx, dy) = (x - self.min_x, y - self.min_y);
        dx >= 0.0
            && dy >= 0.0
            && dx <= self.cols as f32 * self.cell
            && dy <= self.rows as f32 * self.cell
    }

    /// The nearest item to `(x, y)`, by whatever `consider` measures. Exact
    /// distance ties go to the lowest item index, so the answer matches a
    /// linear scan of the item list and does not depend on the grid's layout.
    ///
    /// `consider` is called with an item index and the best squared distance
    /// found so far, and returns that item's squared distance and result, or
    /// `None` to reject it. Passing the running best lets a caller cheaply
    /// discard an item that cannot win before evaluating its geometry -- which
    /// is also what keeps a large item appearing in many cells from being
    /// re-measured in each one. It is an upper bound to beat, not to match:
    /// an item exactly at that distance must still be offered, or the tie
    /// break cannot see it.
    pub fn nearest<T>(
        &self,
        x: f32,
        y: f32,
        mut consider: impl FnMut(u32, f32) -> Option<(f32, T)>,
    ) -> Option<T> {
        if self.cells.is_empty() {
            return None;
        }
        let (cx, cy) = (self.col(x) as isize, self.row(y) as isize);
        let mut best: Option<(f32, u32, T)> = None;
        for r in 0..=self.cols.max(self.rows) as isize {
            // Cells in ring `r` or beyond sit at least `(r - 1)` whole cells
            // away from anywhere inside the query's own cell. Once the best
            // hit is nearer than that, no further ring can improve on it.
            if let Some((d2, _, _)) = &best {
                let floor = (r - 1).max(0) as f32 * self.cell;
                if *d2 <= floor * floor {
                    break;
                }
            }
            let mut in_bounds = false;
            for (ix, iy) in self.ring(cx, cy, r) {
                in_bounds = true;
                for &item in &self.cells[iy * self.cols + ix] {
                    let ceiling = best.as_ref().map_or(f32::INFINITY, |(d2, _, _)| *d2);
                    let Some((d2, value)) = consider(item, ceiling) else {
                        continue;
                    };
                    let wins = best
                        .as_ref()
                        .is_none_or(|(b, bi, _)| (d2, item) < (*b, *bi));
                    if wins {
                        best = Some((d2, item, value));
                    }
                }
            }
            if !in_bounds {
                break; // the ring has outgrown the grid; so will every later one
            }
        }
        best.map(|(_, _, value)| value)
    }

    /// The in-bounds cells at Chebyshev distance `r` from `(cx, cy)`.
    fn ring(&self, cx: isize, cy: isize, r: isize) -> impl Iterator<Item = (usize, usize)> + '_ {
        let (cols, rows) = (self.cols as isize, self.rows as isize);
        (cy - r..=cy + r)
            .filter(move |iy| (0..rows).contains(iy))
            .flat_map(move |iy| {
                // Interior rows contribute only their two end columns; the top
                // and bottom rows of the ring contribute all of theirs.
                let edge = iy == cy - r || iy == cy + r;
                let step = if edge { 1 } else { (2 * r).max(1) };
                (cx - r..=cx + r)
                    .step_by(step as usize)
                    .filter(move |ix| (0..cols).contains(ix))
                    .map(move |ix| (ix as usize, iy as usize))
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One point per item, as a degenerate box.
    fn points(ps: &[(f32, f32)]) -> Vec<Aabb> {
        ps.iter()
            .map(|&(x, y)| Aabb::around([(x, y)]).unwrap())
            .collect()
    }

    /// Brute-force nearest, as the reference answer.
    fn brute(ps: &[(f32, f32)], x: f32, y: f32) -> Option<usize> {
        ps.iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| {
                let d = |p: &(f32, f32)| (p.0 - x).powi(2) + (p.1 - y).powi(2);
                d(a).total_cmp(&d(b))
            })
            .map(|(i, _)| i)
    }

    fn nearest(grid: &Grid, ps: &[(f32, f32)], x: f32, y: f32) -> Option<usize> {
        grid.nearest(x, y, |i, _| {
            let p = ps[i as usize];
            Some(((p.0 - x).powi(2) + (p.1 - y).powi(2), i as usize))
        })
    }

    #[test]
    fn an_empty_grid_finds_nothing() {
        let grid = Grid::build(&[]);
        assert_eq!(nearest(&grid, &[], 0.0, 0.0), None);
    }

    #[test]
    fn agrees_with_brute_force_on_a_scattered_cloud() {
        // A deterministic pseudo-random spread, queried from inside, outside,
        // and far past the grid. The ring walk must never stop early on a
        // candidate a later ring would have beaten.
        let ps: Vec<(f32, f32)> = (0..400)
            .map(|i| {
                let a = i as f32 * 2.399_963; // golden-angle spiral
                (
                    a.cos() * (i as f32).sqrt() * 9.0,
                    a.sin() * (i as f32).sqrt() * 9.0,
                )
            })
            .collect();
        let grid = Grid::build(&points(&ps));
        for k in 0..200 {
            let t = k as f32 * 0.31;
            for (x, y) in [
                (t.cos() * 150.0, t.sin() * 150.0),
                (t * 2.0 - 100.0, t * -1.5 + 60.0),
                (5000.0, -5000.0),
            ] {
                assert_eq!(
                    nearest(&grid, &ps, x, y),
                    brute(&ps, x, y),
                    "disagreed at ({x}, {y})"
                );
            }
        }
    }

    #[test]
    fn a_collinear_cloud_still_indexes() {
        // Zero extent on one axis would divide by zero in the cell sizing.
        let ps: Vec<(f32, f32)> = (0..50).map(|i| (i as f32 * 3.0, 7.0)).collect();
        let grid = Grid::build(&points(&ps));
        for k in 0..50 {
            let x = k as f32 * 3.1 - 10.0;
            assert_eq!(nearest(&grid, &ps, x, 7.0), brute(&ps, x, 7.0));
            assert_eq!(nearest(&grid, &ps, x, 500.0), brute(&ps, x, 500.0));
        }
    }

    #[test]
    fn coincident_items_and_a_single_item_both_resolve() {
        let ps = vec![(1.0, 1.0); 8];
        let grid = Grid::build(&points(&ps));
        assert!(nearest(&grid, &ps, 9.0, 9.0).is_some());

        let one = vec![(4.0, -2.0)];
        let grid = Grid::build(&points(&one));
        assert_eq!(nearest(&grid, &one, 100.0, 100.0), Some(0));
    }

    #[test]
    fn a_non_finite_query_never_panics() {
        let ps = vec![(0.0, 0.0), (10.0, 10.0)];
        let grid = Grid::build(&points(&ps));
        for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let _ = nearest(&grid, &ps, bad, 0.0);
            let _ = nearest(&grid, &ps, 0.0, bad);
        }
    }

    #[test]
    fn a_box_spanning_the_map_is_still_found_from_anywhere() {
        // A single item whose footprint covers everything: it lands in every
        // cell, and the dist2 prune has to stop it being re-measured forever.
        let wide = Aabb {
            min_x: -100.0,
            max_x: 100.0,
            min_y: -100.0,
            max_y: 100.0,
        };
        let mut bounds = points(&[(90.0, 90.0), (-90.0, -90.0)]);
        bounds.push(wide);
        let grid = Grid::build(&bounds);
        let hit = grid.nearest(0.0, 0.0, |i, best| {
            let d2 = bounds[i as usize].dist2(0.0, 0.0);
            (d2 <= best).then_some((d2, i))
        });
        assert_eq!(hit, Some(2), "the enclosing box is at distance zero");
    }

    #[test]
    fn dist2_is_zero_inside_and_grows_outside() {
        let b = Aabb {
            min_x: 0.0,
            max_x: 2.0,
            min_y: 0.0,
            max_y: 2.0,
        };
        assert_eq!(b.dist2(1.0, 1.0), 0.0);
        assert_eq!(b.dist2(2.0, 0.0), 0.0);
        assert_eq!(b.dist2(5.0, 1.0), 9.0);
        assert_eq!(b.dist2(-3.0, -4.0), 25.0);
    }
}
