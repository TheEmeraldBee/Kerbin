use ascii_forge::math::Vec2;

#[derive(Debug, Clone)]
pub enum Constraint {
    Percentage(f32),
    Fixed(u16),
    Range { min: u16, max: u16 },
    Flexible,
}

#[derive(Debug, PartialEq, Eq)]
pub enum LayoutError {
    InsufficientSpace,
    InvalidPercentages,
    ConstraintConflict,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct Rect {
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
}

impl Rect {
    pub fn new(x: u16, y: u16, width: u16, height: u16) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }
}

pub fn percent(value: f32) -> Constraint {
    Constraint::Percentage(value)
}

pub fn fixed(value: u16) -> Constraint {
    Constraint::Fixed(value)
}

pub fn range(min_val: u16, max_val: u16) -> Constraint {
    Constraint::Range {
        min: min_val,
        max: max_val,
    }
}

pub fn flexible() -> Constraint {
    Constraint::Flexible
}

pub struct Layout {
    rows: Vec<(Constraint, Vec<Constraint>)>,
}

impl Layout {
    /// Starts a layout with a total space (usually the window size)
    pub fn new() -> Self {
        Self { rows: Vec::new() }
    }

    /// Creates a row that is then sub-split into other systems
    pub fn row(
        mut self,
        height_constraint: Constraint,
        width_constraints: Vec<Constraint>,
    ) -> Self {
        self.rows.push((height_constraint, width_constraints));
        self
    }

    /// Creates a row that takes up the full row in width.
    pub fn empty_row(self, constraint: Constraint) -> Self {
        self.row(constraint, vec![])
    }

    /// Calculates based on the total space, and gets the Rects for those
    pub fn calculate(self, space: impl Into<Vec2>) -> Result<Vec<Vec<Rect>>, LayoutError> {
        calculate_layout(space, self.rows)
    }
}

pub fn calculate_layout(
    total_space: impl Into<Vec2>,
    rows: Vec<(Constraint, Vec<Constraint>)>,
) -> Result<Vec<Vec<Rect>>, LayoutError> {
    let total_space = total_space.into();
    let height_constraints: Vec<Constraint> = rows.iter().map(|(h, _)| h.clone()).collect();

    let row_heights = resolve_constraints(&height_constraints, total_space.y)?;
    let mut result = Vec::new();
    let mut current_y = 0u16;

    for (row_idx, (_, width_constraints)) in rows.iter().enumerate() {
        let row_height = row_heights[row_idx];
        let widths = resolve_constraints(width_constraints, total_space.x)?;

        let mut row_elements = Vec::new();
        let mut current_x = 0u16;

        for width in widths {
            row_elements.push(Rect::new(current_x, current_y, width, row_height));
            current_x += width;
        }

        result.push(row_elements);
        current_y += row_height;
    }

    Ok(result)
}

pub fn resolve_constraints(
    constraints: &[Constraint],
    available: u16,
) -> Result<Vec<u16>, LayoutError> {
    if constraints.is_empty() {
        return Ok(vec![]);
    }

    let mut total_percentage = 0.0f32;
    for constraint in constraints {
        if let Constraint::Percentage(pct) = constraint {
            if *pct < 0.0 || *pct > 100.0 {
                return Err(LayoutError::InvalidPercentages);
            }
            total_percentage += pct;
        }
    }

    if total_percentage > 100.0 {
        return Err(LayoutError::InvalidPercentages);
    }

    let mut allocated_sizes = vec![0u16; constraints.len()];

    let mut fixed_total = 0u32;
    for (i, constraint) in constraints.iter().enumerate() {
        if let Constraint::Fixed(size) = constraint {
            allocated_sizes[i] = *size;
            fixed_total += *size as u32;
        }
    }

    if fixed_total > available as u32 {
        return Err(LayoutError::InsufficientSpace);
    }

    let mut percentage_total = 0u32;
    for (i, constraint) in constraints.iter().enumerate() {
        if let Constraint::Percentage(pct) = constraint {
            let ideal_size = ((available as f32 * pct) / 100.0).round() as u32;
            allocated_sizes[i] = ideal_size as u16;
            percentage_total += ideal_size;
        }
    }

    if fixed_total + percentage_total > available as u32 {
        let shrink_factor = (available as u32 - fixed_total) as f32 / percentage_total as f32;
        for (i, constraint) in constraints.iter().enumerate() {
            if let Constraint::Percentage(_) = constraint {
                allocated_sizes[i] = (allocated_sizes[i] as f32 * shrink_factor).round() as u16;
            }
        }
    }

    for (i, constraint) in constraints.iter().enumerate() {
        if let Constraint::Range { min: min_val, .. } = constraint {
            allocated_sizes[i] = allocated_sizes[i].max(*min_val);
        }
    }

    let used_space: u32 = allocated_sizes.iter().map(|&x| x as u32).sum();

    if used_space > available as u32 {
        return Err(LayoutError::InsufficientSpace);
    }

    let mut remaining_space = (available as u32) - used_space;

    let flexible_indices: Vec<usize> = constraints
        .iter()
        .enumerate()
        .filter(|(_, constraint)| matches!(constraint, Constraint::Flexible))
        .map(|(i, _)| i)
        .collect();

    let range_indices: Vec<usize> = constraints
        .iter()
        .enumerate()
        .filter(|(_, constraint)| matches!(constraint, Constraint::Range { .. }))
        .map(|(i, _)| i)
        .collect();

    let expandable_indices: Vec<usize> =
        flexible_indices.into_iter().chain(range_indices).collect();

    if !expandable_indices.is_empty() && remaining_space > 0 {
        while remaining_space > 0 {
            let mut distributed = 0u32;
            let eligible_count = expandable_indices
                .iter()
                .filter(|&&idx| {
                    let max_val = match &constraints[idx] {
                        Constraint::Range { max: m, .. } => *m,
                        Constraint::Flexible => u16::MAX,
                        _ => 0,
                    };
                    allocated_sizes[idx] < max_val
                })
                .count();

            if eligible_count == 0 {
                break;
            }

            let space_per_item = std::cmp::max(1, remaining_space / eligible_count as u32);

            for &idx in &expandable_indices {
                let max_val = match &constraints[idx] {
                    Constraint::Range { max: m, .. } => *m,
                    Constraint::Flexible => u16::MAX,
                    _ => 0,
                };

                if allocated_sizes[idx] < max_val && remaining_space > 0 {
                    let can_add = std::cmp::min(
                        max_val.saturating_sub(allocated_sizes[idx]) as u32,
                        std::cmp::min(space_per_item, remaining_space),
                    );
                    allocated_sizes[idx] += can_add as u16;
                    distributed += can_add;
                    remaining_space -= can_add;
                }
            }

            if distributed == 0 {
                break;
            }
        }
    }

    Ok(allocated_sizes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_percent_plus_fixed_heights() {
        let layout_result = Layout::new()
            .row(percent(100.0), vec![percent(100.0)])
            .row(fixed(5), vec![percent(100.0)])
            .calculate((100, 100))
            .unwrap();
        assert_eq!(
            layout_result,
            vec![
                vec![Rect::new(0, 0, 100, 95)],
                vec![Rect::new(0, 95, 100, 5)]
            ]
        );
    }

    #[test]
    fn test_even_flexible_split() {
        let layout_result = Layout::new()
            .row(flexible(), vec![flexible(), flexible()])
            .row(flexible(), vec![flexible(), flexible()])
            .calculate((100, 100))
            .unwrap();
        assert_eq!(
            layout_result,
            vec![
                vec![Rect::new(0, 0, 50, 50), Rect::new(50, 0, 50, 50)],
                vec![Rect::new(0, 50, 50, 50), Rect::new(50, 50, 50, 50)]
            ]
        );
    }

    #[test]
    fn test_mixed_height_fixed_width() {
        let layout_result = Layout::new()
            .row(percent(50.0), vec![fixed(25), fixed(25), flexible()])
            .row(flexible(), vec![fixed(25), fixed(25), flexible()])
            .calculate((100, 100))
            .unwrap();
        assert_eq!(
            layout_result,
            vec![
                vec![
                    Rect::new(0, 0, 25, 50),
                    Rect::new(25, 0, 25, 50),
                    Rect::new(50, 0, 50, 50)
                ],
                vec![
                    Rect::new(0, 50, 25, 50),
                    Rect::new(25, 50, 25, 50),
                    Rect::new(50, 50, 50, 50)
                ]
            ]
        );
    }

    #[test]
    fn test_insufficient_space_fixed_width() {
        let layout_result = Layout::new()
            .row(fixed(10), vec![fixed(60), fixed(60)])
            .calculate((100, 100));
        assert_eq!(layout_result, Err(LayoutError::InsufficientSpace));
    }

    #[test]
    fn test_invalid_percentage_height() {
        let layout_result = Layout::new()
            .row(percent(60.0), vec![percent(100.0)])
            .row(percent(50.0), vec![percent(100.0)])
            .calculate((100, 100));
        assert_eq!(layout_result, Err(LayoutError::InvalidPercentages));
    }

    #[test]
    fn test_complex_single_dimension_mix() {
        let sizes = resolve_constraints(&[fixed(50), percent(25.0), flexible()], 200).unwrap();
        assert_eq!(sizes, vec![50, 50, 100]);
    }

    #[test]
    fn test_all_fixed_exact_fit() {
        let sizes = resolve_constraints(&[fixed(20), fixed(30), fixed(50)], 100).unwrap();
        assert_eq!(sizes, vec![20, 30, 50]);
    }

    #[test]
    fn test_empty_constraints() {
        let sizes = resolve_constraints(&[], 100).unwrap();
        assert_eq!(sizes, vec![]);
    }

    #[test]
    fn test_multiple_flex_with_min_max() {
        let sizes = resolve_constraints(&[range(20, 30), range(10, 70)], 100).unwrap();
        assert_eq!(sizes, vec![30, 70]);
    }

    #[test]
    fn test_range_exceeds_available_min() {
        let sizes = resolve_constraints(&[range(50, 100)], 30);
        assert_eq!(sizes, Err(LayoutError::InsufficientSpace));
    }

    #[test]
    fn test_positions_single_row() {
        let layout_result = Layout::new()
            .row(flexible(), vec![fixed(50), fixed(75), flexible()])
            .calculate((200, 50))
            .unwrap();
        assert_eq!(
            layout_result,
            vec![vec![
                Rect::new(0, 0, 50, 50),
                Rect::new(50, 0, 75, 50),
                Rect::new(125, 0, 75, 50)
            ]]
        );
    }

    #[test]
    fn test_positions_multiple_rows() {
        let layout_result = Layout::new()
            .row(fixed(30), vec![flexible()])
            .row(fixed(20), vec![flexible()])
            .row(flexible(), vec![flexible()])
            .calculate((100, 100))
            .unwrap();
        assert_eq!(
            layout_result,
            vec![
                vec![Rect::new(0, 0, 100, 30)],
                vec![Rect::new(0, 30, 100, 20)],
                vec![Rect::new(0, 50, 100, 50)]
            ]
        );
    }
}
