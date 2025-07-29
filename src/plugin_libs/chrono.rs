use chrono::{Datelike, Duration, Local, NaiveDate, NaiveDateTime, NaiveTime, Timelike, Utc};
use rune::*;

/// Represents a timezone-aware date and time in the local timezone.
#[derive(Any, Clone, Debug)]
pub struct DateTimeLocal {
    inner: chrono::DateTime<Local>,
}

impl DateTimeLocal {
    /// Creates a new `DateTimeLocal` from the current local date and time.
    #[rune::function(keep, path = Self::now)]
    pub fn now() -> Self {
        Self {
            inner: Local::now(),
        }
    }

    /// Returns the year.
    #[rune::function(keep)]
    pub fn year(&self) -> i32 {
        self.inner.year()
    }

    /// Returns the month (1-indexed).
    #[rune::function(keep)]
    pub fn month(&self) -> u32 {
        self.inner.month()
    }

    /// Returns the day of the month (1-indexed).
    #[rune::function(keep)]
    pub fn day(&self) -> u32 {
        self.inner.day()
    }

    /// Returns the hour (0-23).
    #[rune::function(keep)]
    pub fn hour(&self) -> u32 {
        self.inner.hour()
    }

    /// Returns the minute (0-59).
    #[rune::function(keep)]
    pub fn minute(&self) -> u32 {
        self.inner.minute()
    }

    /// Returns the second (0-59).
    #[rune::function(keep)]
    pub fn second(&self) -> u32 {
        self.inner.second()
    }

    /// Formats the `DateTimeLocal` into a string using the given format string.
    #[rune::function(keep)]
    pub fn format(&self, fmt: &str) -> String {
        self.inner.format(fmt).to_string()
    }

    /// Adds a duration to the `DateTimeLocal`.
    #[rune::function(keep)]
    pub fn add_duration(&self, duration: &DurationWrap) -> Self {
        Self {
            inner: self.inner + duration.inner,
        }
    }

    /// Subtracts a duration from the `DateTimeLocal`.
    #[rune::function(keep)]
    pub fn sub_duration(&self, duration: &DurationWrap) -> Self {
        Self {
            inner: self.inner - duration.inner,
        }
    }
}

/// Represents a duration of time.
#[derive(Clone, Debug, Any)]
pub struct DurationWrap {
    inner: Duration,
}

impl DurationWrap {
    /// Creates a new `DurationWrap` from a number of seconds.
    #[rune::function(keep, path=Self::from_seconds)]
    pub fn from_seconds(s: i64) -> Self {
        Self {
            inner: Duration::seconds(s),
        }
    }

    /// Creates a new `DurationWrap` from a number of minutes.
    #[rune::function(keep, path=Self::from_minutes)]
    pub fn from_minutes(m: i64) -> Self {
        Self {
            inner: Duration::minutes(m),
        }
    }

    /// Creates a new `DurationWrap` from a number of hours.
    #[rune::function(keep, path=Self::from_hours)]
    pub fn from_hours(h: i64) -> Self {
        Self {
            inner: Duration::hours(h),
        }
    }

    /// Creates a new `DurationWrap` from a number of days.
    #[rune::function(keep, path=Self::from_days)]
    pub fn from_days(d: i64) -> Self {
        Self {
            inner: Duration::days(d),
        }
    }
}

pub fn module() -> Result<Module, ContextError> {
    let mut module = Module::with_crate("chrono")?;

    module.ty::<DateTimeLocal>()?;
    module.function_meta(DateTimeLocal::now__meta)?;
    module.function_meta(DateTimeLocal::year__meta)?;
    module.function_meta(DateTimeLocal::month__meta)?;
    module.function_meta(DateTimeLocal::day__meta)?;
    module.function_meta(DateTimeLocal::hour__meta)?;
    module.function_meta(DateTimeLocal::minute__meta)?;
    module.function_meta(DateTimeLocal::second__meta)?;
    module.function_meta(DateTimeLocal::format__meta)?;
    module.function_meta(DateTimeLocal::add_duration__meta)?;
    module.function_meta(DateTimeLocal::sub_duration__meta)?;

    module.ty::<DurationWrap>()?;
    module.function_meta(DurationWrap::from_seconds__meta)?;
    module.function_meta(DurationWrap::from_minutes__meta)?;
    module.function_meta(DurationWrap::from_hours__meta)?;
    module.function_meta(DurationWrap::from_days__meta)?;

    Ok(module)
}
