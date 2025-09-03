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
    total_space: (u16, u16),
    rows: Vec<(Constraint, Vec<Constraint>)>,
}

impl Layout {
    pub fn new(total_space: (u16, u16)) -> Self {
        Self {
            total_space,
            rows: Vec::new(),
        }
    }

    pub fn add_row(
        mut self,
        height_constraint: Constraint,
        width_constraints: Vec<Constraint>,
    ) -> Self {
        self.rows.push((height_constraint, width_constraints));
        self
    }

    pub fn row(self, height_constraint: Constraint, width_constraints: Vec<Constraint>) -> Self {
        self.add_row(height_constraint, width_constraints)
    }

    pub fn calculate(self) -> Result<Vec<Vec<(u16, u16)>>, LayoutError> {
        calculate_layout(self.total_space, self.rows)
    }
}

pub fn calculate_layout(
    total_space: (u16, u16),
    rows: Vec<(Constraint, Vec<Constraint>)>,
) -> Result<Vec<Vec<(u16, u16)>>, LayoutError> {
    let height_constraints: Vec<Constraint> = rows.iter().map(|(h, _)| h.clone()).collect();

    let row_heights = resolve_constraints(&height_constraints, total_space.1)?;
    let mut result = Vec::new();

    for (row_idx, (_, width_constraints)) in rows.iter().enumerate() {
        let row_height = row_heights[row_idx];
        let widths = resolve_constraints(width_constraints, total_space.0)?;

        let row_elements: Vec<(u16, u16)> =
            widths.iter().map(|&width| (width, row_height)).collect();

        result.push(row_elements);
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

    let expandable_indices: Vec<usize> = flexible_indices
        .into_iter()
        .chain(range_indices.into_iter())
        .collect();

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

fn main() {
    let layout_result = Layout::new((320, 200))
        .row(percent(30.0), vec![percent(50.0), fixed(5), range(5, 50)])
        .row(percent(70.0), vec![percent(100.0)])
        .calculate();

    match layout_result {
        Ok(rows) => {
            println!("Layout result:");
            for (row_idx, row) in rows.iter().enumerate() {
                println!("Row {}: {:?}", row_idx, row);
            }
        }
        Err(e) => println!("Layout failed: {:?}", e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_percent_plus_fixed_heights() {
        let layout_result = Layout::new((100, 100))
            .row(percent(100.0), vec![percent(100.0)])
            .row(fixed(5), vec![percent(100.0)])
            .calculate()
            .unwrap();
        assert_eq!(layout_result, vec![vec![(100, 95)], vec![(100, 5)]]);
    }

    #[test]
    fn test_even_flexible_split() {
        let layout_result = Layout::new((100, 100))
            .row(flexible(), vec![flexible(), flexible()])
            .row(flexible(), vec![flexible(), flexible()])
            .calculate()
            .unwrap();
        assert_eq!(
            layout_result,
            vec![vec![(50, 50), (50, 50)], vec![(50, 50), (50, 50)]]
        );
    }

    #[test]
    fn test_mixed_height_fixed_width() {
        let layout_result = Layout::new((100, 100))
            .row(percent(50.0), vec![fixed(25), fixed(25), flexible()])
            .row(flexible(), vec![fixed(25), fixed(25), flexible()])
            .calculate()
            .unwrap();
        assert_eq!(
            layout_result,
            vec![
                vec![(25, 50), (25, 50), (50, 50)],
                vec![(25, 50), (25, 50), (50, 50)]
            ]
        );
    }

    #[test]
    fn test_insufficient_space_fixed_width() {
        let layout_result = Layout::new((100, 100))
            .row(fixed(10), vec![fixed(60), fixed(60)])
            .calculate();
        assert_eq!(layout_result, Err(LayoutError::InsufficientSpace));
    }

    #[test]
    fn test_invalid_percentage_height() {
        let layout_result = Layout::new((100, 100))
            .row(percent(60.0), vec![percent(100.0)])
            .row(percent(50.0), vec![percent(100.0)])
            .calculate();
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
}
