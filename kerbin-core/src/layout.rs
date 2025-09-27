use ascii_forge::math::Vec2;

/// Defines a constraint for sizing elements within a layout.
///
/// Constraints determine how much space an element should occupy relative
/// to the available space or other elements.
#[derive(Debug, Clone)]
pub enum Constraint {
    /// Takes up a specified percentage of the total available space (0.0 to 100.0).
    /// It will shrink if necessary to fit within the available space.
    Percentage(f32),
    /// Takes up a fixed amount of space in units (e.g., characters or rows).
    /// If the available space is less than the fixed size, an error may occur.
    Fixed(u16),
    /// Takes up space within a specified minimum and maximum range.
    /// It will try to fit its content but won't go below `min` or above `max`.
    Range { min: u16, max: u16 },
    /// Takes up all the remaining available space after other constraints have been resolved.
    /// Multiple flexible constraints will share the remaining space evenly.
    Flexible,
}

/// The possible error results that can occur during layout calculation.
#[derive(Debug, PartialEq, Eq)]
pub enum LayoutError {
    /// Indicates that at least one constraint (e.g., a `Fixed` or `Range` with too high `min`)
    /// could not fit within the allocated space.
    InsufficientSpace,

    /// Occurs when `Percentage` constraints sum up to more than 100%, or a percentage
    /// value is outside the 0.0-100.0 range.
    InvalidPercentages,

    /// Reserved for potential future conflicts where constraints are logically impossible
    /// to satisfy simultaneously (currently not explicitly triggered by `resolve_constraints`).
    ConstraintConflict,
}

/// An area that a layout element takes up.
///
/// Represents a rectangular region on the screen, defined by its top-left
/// corner (x, y) and its dimensions (width, height).
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct Rect {
    /// The X-coordinate of the top-left corner.
    pub x: u16,
    /// The Y-coordinate of the top-left corner.
    pub y: u16,
    /// The width of the rectangle.
    pub width: u16,
    /// The height of the rectangle.
    pub height: u16,
}

impl Rect {
    /// Creates a new `Rect` with the specified position and dimensions.
    ///
    /// # Arguments
    ///
    /// * `x`: The X-coordinate of the top-left corner.
    /// * `y`: The Y-coordinate of the top-left corner.
    /// * `width`: The width of the rectangle.
    /// * `height`: The height of the rectangle.
    ///
    /// # Returns
    ///
    /// A new `Rect` instance.
    pub fn new(x: u16, y: u16, width: u16, height: u16) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }
}

/// Creates a `Constraint::Percentage` variant.
///
/// This helper function is a convenient way to specify a percentage-based constraint.
/// The `value` should be between 0.0 and 100.0.
///
/// # Arguments
///
/// * `value`: The percentage as a `f32`.
///
/// # Returns
///
/// A `Constraint::Percentage` instance.
pub fn percent(value: f32) -> Constraint {
    Constraint::Percentage(value)
}

/// Creates a `Constraint::Fixed` variant.
///
/// This helper function specifies that an element should occupy an exact
/// fixed amount of space.
///
/// # Arguments
///
/// * `value`: The fixed size in units as a `u16`.
///
/// # Returns
///
/// A `Constraint::Fixed` instance.
pub fn fixed(value: u16) -> Constraint {
    Constraint::Fixed(value)
}

/// Creates a `Constraint::Range` variant.
///
/// This helper function specifies that an element should occupy space within
/// a given minimum and maximum bound.
///
/// # Arguments
///
/// * `min_val`: The minimum allowed size in units as a `u16`.
/// * `max_val`: The maximum allowed size in units as a `u16`.
///
/// # Returns
///
/// A `Constraint::Range` instance.
pub fn range(min_val: u16, max_val: u16) -> Constraint {
    Constraint::Range {
        min: min_val,
        max: max_val,
    }
}

/// Creates a `Constraint::Flexible` variant.
///
/// This helper function specifies that an element should take up any
/// remaining space.
///
/// # Returns
///
/// A `Constraint::Flexible` instance.
pub fn flexible() -> Constraint {
    Constraint::Flexible
}

/// Defines a horizontal and vertical grid layout setup.
///
/// `Layout` is used for separating a given total space (e.g., the window size)
/// into easy-to-manage rectangular chunks for rendering UI elements.
#[derive(Default)]
pub struct Layout {
    /// A vector where each tuple represents a row: `(height_constraint, width_constraints_for_columns)`.
    rows: Vec<(Constraint, Vec<Constraint>)>,
}

impl Layout {
    /// Starts a new `Layout` definition.
    ///
    /// # Returns
    ///
    /// An empty `Layout` instance.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a new row to the layout with specified height and column width constraints.
    ///
    /// # Arguments
    ///
    /// * `height_constraint`: The `Constraint` that determines the height of this row.
    /// * `width_constraints`: A `Vec<Constraint>` where each constraint determines the width
    ///   of a column within this row.
    ///
    /// # Returns
    ///
    /// The `Layout` instance, allowing for method chaining.
    pub fn row(
        mut self,
        height_constraint: Constraint,
        width_constraints: Vec<Constraint>,
    ) -> Self {
        self.rows.push((height_constraint, width_constraints));
        self
    }

    /// Creates a row that takes up the full width of the available space with a single height constraint.
    ///
    /// This is a convenience method for `row` when a row only contains one conceptual column
    /// that spans the entire width.
    ///
    /// # Arguments
    ///
    /// * `constraint`: The `Constraint` that determines the height of this row.
    ///
    /// # Returns
    ///
    /// The `Layout` instance, allowing for method chaining.
    pub fn empty_row(self, constraint: Constraint) -> Self {
        self.row(constraint, vec![flexible()]) // A single flexible width constraint
    }

    /// Calculates the `Rect`s for all elements in the layout based on the total available space.
    ///
    /// This method consumes the `Layout` instance and computes the final rectangular
    /// areas for all rows and columns.
    ///
    /// # Arguments
    ///
    /// * `space`: The total `Vec2` representing the available width and height for the layout.
    ///   Can be any type that converts into `Vec2`.
    ///
    /// # Returns
    ///
    /// A `Result` which is:
    /// - `Ok(Vec<Vec<Rect>>)`: A nested vector where the outer vector corresponds to rows,
    ///   and the inner vectors contain the `Rect`s for the columns within that row.
    /// - `Err(LayoutError)`: If the constraints cannot be satisfied (e.g., insufficient space).
    pub fn calculate(self, space: impl Into<Vec2>) -> Result<Vec<Vec<Rect>>, LayoutError> {
        calculate_layout(space, self.rows)
    }
}

/// Calculates the layout of a grid, resolving constraints for rows and columns.
///
/// This is the core logic for the `Layout::calculate` method. It first resolves
/// height constraints for all rows, then for each row, resolves the width constraints
/// for its columns.
///
/// # Arguments
///
/// * `total_space`: The total `Vec2` representing the available width and height.
/// * `rows`: A vector of tuples, each containing a `Constraint` for row height
///   and a `Vec<Constraint>` for column widths within that row.
///
/// # Returns
///
/// A `Result` which is:
/// - `Ok(Vec<Vec<Rect>>)`: A nested vector of calculated `Rect`s.
/// - `Err(LayoutError)`: If constraints cannot be resolved.
pub fn calculate_layout(
    total_space: impl Into<Vec2>,
    rows: Vec<(Constraint, Vec<Constraint>)>,
) -> Result<Vec<Vec<Rect>>, LayoutError> {
    let total_space = total_space.into();
    let height_constraints: Vec<Constraint> = rows.iter().map(|(h, _)| h.clone()).collect();

    // Resolve heights for all rows
    let row_heights = resolve_constraints(&height_constraints, total_space.y)?;
    let mut result = Vec::new();
    let mut current_y = 0u16;

    // Iterate through rows to resolve column widths and create Rects
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

/// Resolves a list of `Constraint`s for a single dimension (either width or height).
///
/// This function attempts to distribute `available` space among a set of constraints,
/// adhering to fixed sizes, percentages, ranges, and flexible space distribution.
///
/// The resolution order is generally:
/// 1. Validate percentages.
/// 2. Allocate fixed sizes.
/// 3. Allocate percentage sizes (with potential shrinking if total > available).
/// 4. Ensure minimums of `Range` constraints are met.
/// 5. Distribute remaining space to `Flexible` and `Range` (up to their max) constraints.
///
/// # Arguments
///
/// * `constraints`: A slice of `Constraint`s to resolve for the given dimension.
/// * `available`: The total `u16` space available in that dimension.
///
/// # Returns
///
/// A `Result` which is:
/// - `Ok(Vec<u16>)`: A vector of calculated sizes for each constraint, summing up to `available`.
/// - `Err(LayoutError)`: If the constraints cannot be satisfied (e.g., not enough space, invalid percentages).
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

    // If combined fixed and percentage exceeds available, shrink percentages proportionally
    if fixed_total + percentage_total > available as u32 {
        let shrink_factor = (available as u32 - fixed_total) as f32 / percentage_total as f32;
        for (i, constraint) in constraints.iter().enumerate() {
            if let Constraint::Percentage(_) = constraint {
                allocated_sizes[i] = (allocated_sizes[i] as f32 * shrink_factor).round() as u16;
            }
        }
    }

    // Ensure range minimums are met
    for (i, constraint) in constraints.iter().enumerate() {
        if let Constraint::Range { min: min_val, .. } = constraint {
            allocated_sizes[i] = allocated_sizes[i].max(*min_val);
        }
    }

    let used_space: u32 = allocated_sizes.iter().map(|&x| x as u32).sum();

    if used_space > available as u32 {
        // After ensuring minimums, if we've exceeded space, it's an error.
        // This could happen if fixed + initial percentages + range mins > available
        return Err(LayoutError::InsufficientSpace);
    }

    let mut remaining_space = (available as u32) - used_space;

    // Identify indices of flexible and range constraints for expansion
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

    // Distribute remaining space to flexible and range constraints (up to their max)
    if !expandable_indices.is_empty() && remaining_space > 0 {
        while remaining_space > 0 {
            let mut distributed = 0u32;
            let eligible_count = expandable_indices
                .iter()
                .filter(|&&idx| {
                    let max_val = match &constraints[idx] {
                        Constraint::Range { max: m, .. } => *m,
                        Constraint::Flexible => u16::MAX, // Flexible has no upper bound
                        _ => 0, // Other constraints are not expandable here
                    };
                    allocated_sizes[idx] < max_val // Only expand if not yet at max
                })
                .count();

            if eligible_count == 0 {
                break; // No more eligible items to expand, or remaining_space is 0
            }

            // Distribute space as evenly as possible, ensuring at least 1 unit per item
            let space_per_item = std::cmp::max(1, remaining_space / eligible_count as u32);

            for &idx in &expandable_indices {
                let max_val = match &constraints[idx] {
                    Constraint::Range { max: m, .. } => *m,
                    Constraint::Flexible => u16::MAX,
                    _ => 0,
                };

                if allocated_sizes[idx] < max_val && remaining_space > 0 {
                    // Calculate how much can be added to this item without exceeding its max
                    // or the remaining overall space, or the per-item distribution amount.
                    let can_add = std::cmp::min(
                        max_val.saturating_sub(allocated_sizes[idx]) as u32, // Space left till max
                        std::cmp::min(space_per_item, remaining_space),      // Space to distribute
                    );
                    allocated_sizes[idx] += can_add as u16;
                    distributed += can_add;
                    remaining_space -= can_add;
                }
            }

            if distributed == 0 {
                // If no space was distributed in an iteration but `remaining_space` > 0,
                // it means no more items can expand (e.g., all hit their max).
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
