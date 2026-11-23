use serde::{Deserialize, Serialize};

/// The system local time that a time rule reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LocalTime {
    /// ISO weekday: 1 Monday to 7 Sunday.
    pub weekday: u8,
    /// Minutes after midnight, 0 to 1439.
    pub minute: u16,
}

pub const MINUTES_PER_DAY: u16 = 24 * 60;

/// Weekdays with a time range. `end` is exclusive. When `end` is before `start`, the range ends
/// on the next day and belongs to the day of `start`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TimeRule {
    /// ISO weekdays, 1 Monday to 7 Sunday.
    pub days: Vec<u8>,
    pub start: u16,
    pub end: u16,
}

impl TimeRule {
    pub fn validate(&self) -> Result<(), String> {
        if self.days.iter().any(|d| !(1..=7).contains(d)) {
            return Err("days are ISO weekdays, 1 to 7".into());
        }
        if self.start >= MINUTES_PER_DAY || self.end > MINUTES_PER_DAY {
            return Err("start and end are minutes after midnight".into());
        }
        Ok(())
    }

    fn has_day(&self, day: u8) -> bool {
        self.days.contains(&day)
    }

    pub fn holds(&self, now: LocalTime) -> bool {
        if self.start <= self.end {
            return self.has_day(now.weekday) && (self.start..self.end).contains(&now.minute);
        }
        // The range crosses midnight. The part after midnight belongs to the day before.
        let yesterday = if now.weekday == 1 { 7 } else { now.weekday - 1 };
        (self.has_day(now.weekday) && now.minute >= self.start)
            || (self.has_day(yesterday) && now.minute < self.end)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(weekday: u8, hour: u16, minute: u16) -> LocalTime {
        LocalTime {
            weekday,
            minute: hour * 60 + minute,
        }
    }

    #[test]
    fn time_range_across_midnight_belongs_to_start_day() {
        // Friday and Saturday, 22:00 to 02:00.
        let rule = TimeRule {
            days: vec![5, 6],
            start: 22 * 60,
            end: 2 * 60,
        };
        assert!(rule.holds(at(5, 22, 0)));
        assert!(rule.holds(at(5, 23, 59)));
        // Saturday 01:00 belongs to the range of Friday.
        assert!(rule.holds(at(6, 1, 0)));
        // Saturday 22:00 starts the range of Saturday, and Sunday 01:00 ends it.
        assert!(rule.holds(at(6, 22, 0)));
        assert!(rule.holds(at(7, 1, 59)));
        assert!(!rule.holds(at(7, 2, 0)));
        // Friday 01:00 belongs to Thursday, which the rule does not list.
        assert!(!rule.holds(at(5, 1, 0)));
        assert!(!rule.holds(at(5, 12, 0)));
        // Sunday 22:00 starts a range on a day that the rule does not list.
        assert!(!rule.holds(at(7, 22, 0)));
        // Monday 01:00 belongs to Sunday: not listed.
        assert!(!rule.holds(at(1, 1, 0)));
    }

    #[test]
    fn time_weekday_iso() {
        // Monday is 1 and Sunday is 7. The week wraps from Sunday to Monday.
        let rule = TimeRule {
            days: vec![1, 7],
            start: 0,
            end: MINUTES_PER_DAY,
        };
        assert!(rule.holds(at(1, 12, 0)));
        assert!(rule.holds(at(7, 12, 0)));
        for day in 2..=6 {
            assert!(!rule.holds(at(day, 12, 0)), "day {day}");
        }
        // A range on Sunday that crosses midnight ends on Monday.
        let sunday = TimeRule {
            days: vec![7],
            start: 23 * 60,
            end: 60,
        };
        assert!(sunday.holds(at(1, 0, 30)));
        assert!(!sunday.holds(at(2, 0, 30)));
        // A same-day range is half open.
        let day = TimeRule {
            days: vec![3],
            start: 8 * 60,
            end: 17 * 60,
        };
        assert!(day.holds(at(3, 8, 0)));
        assert!(!day.holds(at(3, 17, 0)));
        assert!(
            TimeRule {
                days: vec![0],
                ..day.clone()
            }
            .validate()
            .is_err()
        );
        assert!(TimeRule { start: 1440, ..day }.validate().is_err());
    }
}
