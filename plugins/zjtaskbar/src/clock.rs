//! The tray's clock: the time of day, drawn from a format with
//! strftime-like specifiers, kept to the local time and ticking only as
//! often as the format shows.
//!
//! A plugin runs in a WebAssembly sandbox with no time zone database and no
//! way to read the system's own setting, so the clock works in UTC, from
//! `Host::system_time`, and adds an offset to it:
//!
//! - `clock_utc_offset`, such as `+02:00` or `-0530`, when the user sets one.
//!   One that does not parse is ignored, and the clock keeps UTC.
//! - Otherwise the system's own, from running `date +%z` (see
//!   `CLOCK_COMMAND`). That needs Zellij's `RunCommands` permission, and the
//!   bar asks for it only in this case: see `host::Feature::ClockCommand`.
//!   The command runs once the permission is granted and again in each new
//!   hour, so a change of daylight saving time shows within the hour. Until
//!   its first answer the clock draws nothing, rather than a time that may be
//!   hours off. An answer that does not parse keeps the offset known so far,
//!   or UTC when none is; a denied permission means UTC.
//!
//! The clock is redrawn by a timer, `Host::set_timeout`, armed for the next
//! moment the drawn text can change: the next second while the format shows
//! seconds, and otherwise the next minute. Zellij cannot cancel a timer, and
//! tells a plugin only how long one took to fire, not which one it was, so
//! the bar keeps when each timer it armed is due and arms another only when
//! none of those comes by the next tick; see `State::follow_clock`.
//!
//! The calendar is worked out here, from the count of days since the Unix
//! epoch, so the plugin needs no date library: see `CivilTime::at`.

use crate::tray::TrayItem;
use crate::State;
use std::collections::BTreeMap;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use zellij_tile::prelude::{PermissionStatus, PermissionType};
use zjkit::host::Host;

/// The command that prints the system's offset of the local time from UTC,
/// as `+0200` or `-0530`. `%z` is in POSIX `date`, so it is there on every
/// system Zellij runs on.
const CLOCK_COMMAND: [&str; 2] = ["date", "+%z"];

/// The context the bar sends with `CLOCK_COMMAND`, which Zellij hands back
/// in its `RunCommandResult`, so the bar can tell the answer from any other
/// command's.
const CLOCK_CONTEXT_KEY: &str = "zjtaskbar";
const CLOCK_CONTEXT_VALUE: &str = "clock_utc_offset";

/// How long after the moment the text changes the timer fires. Zellij's
/// timer may fire a little early, and a tick that came just before the new
/// minute would draw the old one again and leave the new one a whole minute
/// late. A hundredth of a second is too short to see.
const TICK_MARGIN_SECONDS: f64 = 0.01;

/// Seconds in a minute, an hour and a day.
const MINUTE: i64 = 60;
const HOUR: i64 = 60 * MINUTE;
const DAY: i64 = 24 * HOUR;

/// The English names of the weekdays, from Sunday, and of the months, from
/// January, as the C locale's `%A` and `%B` print them. `%a` and `%b` take
/// their first three letters.
const WEEKDAYS: [&str; 7] = [
    "Sunday",
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
];
const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

/// How far the local time is ahead of UTC, east of Greenwich, or behind it
/// when negative.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct UtcOffset {
    minutes: i32,
}

impl UtcOffset {
    pub(crate) const UTC: UtcOffset = UtcOffset { minutes: 0 };

    /// The offset `text` writes, as `+HH:MM`, `+HHMM` or `+HH`, with `-` for
    /// one behind UTC: `+02:00`, `-0530`, `+09`. The sign and two digits of
    /// hours are required, the hours below 24 and the minutes below 60.
    /// Spaces around it are ignored, so `date`'s line ending is too. `None`
    /// for anything else.
    pub(crate) fn parse(text: &str) -> Option<UtcOffset> {
        let text = text.trim();
        let (sign, rest) = match text.as_bytes().first()? {
            b'+' => (1, &text[1..]),
            b'-' => (-1, &text[1..]),
            _ => return None,
        };
        // Only ASCII can be an offset, and checking first keeps the slices
        // below on character boundaries whatever was written.
        if !rest.is_ascii() {
            return None;
        }
        let digits = |part: &str| {
            (part.len() == 2 && part.bytes().all(|byte| byte.is_ascii_digit()))
                .then(|| part.parse::<i32>().ok())
                .flatten()
        };
        let (hours, minutes) = match rest.len() {
            2 => (digits(rest)?, 0),
            4 => (digits(&rest[..2])?, digits(&rest[2..])?),
            5 if rest.as_bytes()[2] == b':' => (digits(&rest[..2])?, digits(&rest[3..])?),
            _ => return None,
        };
        if hours >= 24 || minutes >= 60 {
            return None;
        }
        Some(UtcOffset {
            minutes: sign * (hours * 60 + minutes),
        })
    }

    fn seconds(self) -> i64 {
        i64::from(self.minutes) * MINUTE
    }

    /// The offset as `%z` prints it, `+0200` or `-0530`.
    fn to_numeric(self) -> String {
        let sign = if self.minutes < 0 { '-' } else { '+' };
        let minutes = self.minutes.abs();
        format!("{sign}{:02}{:02}", minutes / 60, minutes % 60)
    }
}

/// Where the clock takes its offset from UTC, from `clock_utc_offset`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum OffsetSource {
    /// The system's, from `CLOCK_COMMAND`, while the option is unset or
    /// empty.
    #[default]
    System,
    /// The user's own, or UTC when what they wrote does not parse.
    Fixed(UtcOffset),
}

impl OffsetSource {
    /// Where `clock_utc_offset`, as the user wrote it, says to take the
    /// offset from.
    pub(crate) fn of(option: Option<&str>) -> OffsetSource {
        match option {
            None => OffsetSource::System,
            Some(text) => OffsetSource::Fixed(UtcOffset::parse(text).unwrap_or(UtcOffset::UTC)),
        }
    }
}

/// A moment as a wall clock and a calendar show it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CivilTime {
    pub year: i64,
    /// From 1, January, to 12.
    pub month: u32,
    /// The day of the month, from 1.
    pub day: u32,
    /// From 0 to 23.
    pub hour: u32,
    pub minute: u32,
    pub second: u32,
    /// From 0, Sunday, to 6, Saturday.
    pub weekday: u32,
    /// The day of the year, from 1, January 1st, to 366.
    pub day_of_year: u32,
}

impl CivilTime {
    /// The moment `seconds` after the Unix epoch, 1970-01-01 00:00:00, in a
    /// calendar whose midnight is at that epoch's: UTC for a UTC count, and
    /// local time for one the offset is already added to. A negative count
    /// is before the epoch.
    pub(crate) fn at(seconds: i64) -> CivilTime {
        let days = seconds.div_euclid(DAY);
        let of_day = seconds.rem_euclid(DAY);
        let (year, month, day) = civil_date(days);
        let day_of_year = days - days_from_civil_year_start(year) + 1;
        CivilTime {
            year,
            month,
            day,
            hour: (of_day / HOUR) as u32,
            minute: (of_day % HOUR / MINUTE) as u32,
            second: (of_day % MINUTE) as u32,
            // The epoch was a Thursday.
            weekday: (days + 4).rem_euclid(7) as u32,
            day_of_year: day_of_year as u32,
        }
    }

    /// The hour, from 1 to 12, as a 12-hour clock shows it.
    fn hour_of_twelve(self) -> u32 {
        match self.hour % 12 {
            0 => 12,
            hour => hour,
        }
    }
}

/// The year, month and day of the date `days` after 1970-01-01, in the
/// proleptic Gregorian calendar, which runs the leap year rule back before it
/// was adopted.
///
/// The calendar repeats every 400 years, which are 146097 days, so the date
/// is found within its 400-year cycle. Each cycle is counted from March 1st,
/// so that February 29th, when there is one, ends the year rather than
/// falling in the middle of it: then every year of the cycle starts the same
/// way, and only its length differs. Counted from March, the months have the
/// lengths 31, 30, 31, 30, 31, 31, 30, 31, 30, 31, 31 and whatever February
/// has, which `(153 * month + 2) / 5` turns into the day each starts on.
fn civil_date(days: i64) -> (i64, u32, u32) {
    // Days from 0000-03-01 to 1970-01-01.
    const EPOCH_FROM_MARCH_OF_YEAR_ZERO: i64 = 719_468;
    const DAYS_IN_CYCLE: i64 = 146_097;
    let from_year_zero = days + EPOCH_FROM_MARCH_OF_YEAR_ZERO;
    let cycle = from_year_zero.div_euclid(DAYS_IN_CYCLE);
    let day_of_cycle = from_year_zero.rem_euclid(DAYS_IN_CYCLE);
    // A year of the cycle is 365 days, plus one for each fourth year, less
    // one for each hundredth, plus one for the four hundredth. Taking those
    // days back out makes every year 365 days long.
    let year_of_cycle = (day_of_cycle - day_of_cycle / 1460 + day_of_cycle / 36_524
        - day_of_cycle / (DAYS_IN_CYCLE - 1))
        / 365;
    let day_of_year_from_march =
        day_of_cycle - (365 * year_of_cycle + year_of_cycle / 4 - year_of_cycle / 100);
    let month_from_march = (5 * day_of_year_from_march + 2) / 153;
    let day = day_of_year_from_march - (153 * month_from_march + 2) / 5 + 1;
    let month = if month_from_march < 10 {
        month_from_march + 3
    } else {
        month_from_march - 9
    };
    // January and February belong to the year after the March they follow.
    let year = cycle * 400 + year_of_cycle + i64::from(month <= 2);
    (year, month as u32, day as u32)
}

/// The days from 1970-01-01 to January 1st of `year`, negative before 1970.
fn days_from_civil_year_start(year: i64) -> i64 {
    // The leap days in the years before `year`, counted from year 0, which
    // is itself a leap year.
    let leap_days_before = |year: i64| {
        let previous = year - 1;
        previous.div_euclid(4) - previous.div_euclid(100) + previous.div_euclid(400) + 1
    };
    365 * (year - 1970) + leap_days_before(year) - leap_days_before(1970)
}

/// One piece of a clock format: text drawn as it is, or a specifier.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Piece<'a> {
    Text(&'a str),
    /// The letter after a `%`.
    Specifier(char),
}

/// The specifiers the clock knows, each the letter after its `%`.
const SPECIFIERS: &str = "HMSIpYymdeajAbBz%";

/// The pieces of `format`, in order. A `%` followed by a letter the clock
/// does not know, or ending the format, is text, so it is drawn as typed.
fn pieces(format: &str) -> Vec<Piece<'_>> {
    let mut pieces = Vec::new();
    let mut text_start = 0;
    let mut chars = format.char_indices().peekable();
    while let Some((index, char)) = chars.next() {
        if char != '%' {
            continue;
        }
        let Some(&(_, letter)) = chars.peek() else {
            break;
        };
        if !SPECIFIERS.contains(letter) {
            continue;
        }
        chars.next();
        if text_start < index {
            pieces.push(Piece::Text(&format[text_start..index]));
        }
        pieces.push(Piece::Specifier(letter));
        text_start = index + 1 + letter.len_utf8();
    }
    if text_start < format.len() {
        pieces.push(Piece::Text(&format[text_start..]));
    }
    pieces
}

/// How often a clock format's text changes, and so how often the clock
/// ticks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Precision {
    /// The format shows seconds, with `%S`.
    Second,
    /// Every other format. One that shows no minutes, such as a date alone,
    /// still ticks each minute, so a change of format or of offset shows
    /// within a minute too.
    Minute,
}

impl Precision {
    pub(crate) fn of(format: &str) -> Precision {
        if pieces(format).contains(&Piece::Specifier('S')) {
            Precision::Second
        } else {
            Precision::Minute
        }
    }

    fn seconds(self) -> i64 {
        match self {
            Precision::Second => 1,
            Precision::Minute => MINUTE,
        }
    }

    /// How long from `now` until the next moment a format of this precision
    /// can draw differently, with `TICK_MARGIN_SECONDS` after it. Offsets
    /// are whole minutes, so that moment is the same in UTC as in local
    /// time.
    pub(crate) fn seconds_until_next_tick(self, now: SystemTime) -> f64 {
        let (seconds, nanoseconds) = unix_time(now);
        let into_period = seconds.rem_euclid(self.seconds()) as f64 + nanoseconds as f64 / 1e9;
        self.seconds() as f64 - into_period + TICK_MARGIN_SECONDS
    }
}

/// `time` as whole seconds since the Unix epoch, negative before it, and
/// the nanoseconds past that second.
fn unix_time(time: SystemTime) -> (i64, u32) {
    match time.duration_since(UNIX_EPOCH) {
        Ok(since) => (since.as_secs() as i64, since.subsec_nanos()),
        Err(error) => {
            let before = error.duration();
            let seconds = -(before.as_secs() as i64);
            match before.subsec_nanos() {
                0 => (seconds, 0),
                nanoseconds => (seconds - 1, 1_000_000_000 - nanoseconds),
            }
        }
    }
}

/// `format` with each specifier replaced by its part of `time`, the local
/// time at `offset` from UTC. See `docs/zjtaskbar/tray.md` for the table of
/// specifiers.
pub(crate) fn format_time(format: &str, time: CivilTime, offset: UtcOffset) -> String {
    let mut text = String::new();
    for piece in pieces(format) {
        match piece {
            Piece::Text(part) => text.push_str(part),
            Piece::Specifier(letter) => text.push_str(&specifier(letter, time, offset)),
        }
    }
    text
}

/// What the specifier `%letter` draws for `time`.
fn specifier(letter: char, time: CivilTime, offset: UtcOffset) -> String {
    let weekday = WEEKDAYS[time.weekday as usize];
    let month = MONTHS[time.month as usize - 1];
    match letter {
        'H' => format!("{:02}", time.hour),
        'M' => format!("{:02}", time.minute),
        'S' => format!("{:02}", time.second),
        'I' => format!("{:02}", time.hour_of_twelve()),
        'p' => if time.hour < 12 { "AM" } else { "PM" }.to_string(),
        'Y' => time.year.to_string(),
        'y' => format!("{:02}", time.year.rem_euclid(100)),
        'm' => format!("{:02}", time.month),
        'd' => format!("{:02}", time.day),
        'e' => format!("{:>2}", time.day),
        'j' => format!("{:03}", time.day_of_year),
        'a' => weekday[..3].to_string(),
        'A' => weekday.to_string(),
        'b' => month[..3].to_string(),
        'B' => month.to_string(),
        'z' => offset.to_numeric(),
        '%' => "%".to_string(),
        _ => unreachable!("`pieces` makes a specifier only of a letter in SPECIFIERS"),
    }
}

/// What the tray's clock knows and keeps between ticks.
#[derive(Debug, Default)]
pub(crate) struct Clock {
    /// The format of the time, with its specifiers, from `clock_format`.
    pub format: String,
    /// The format string the time is drawn in, with `{clock}`, or `None` for
    /// Zellij's own ribbon colors, from `clock_item_format`.
    pub item_format: Option<String>,
    /// Where the offset from UTC comes from, from `clock_utc_offset`.
    pub offset_source: OffsetSource,
    /// The system's offset, from the last answer of `CLOCK_COMMAND` that
    /// parsed, or UTC once a permission was denied or an answer did not
    /// parse with none known. `None` until then.
    pub system_offset: Option<UtcOffset>,
    /// Whether Zellij's last answer to the bar's permission request granted
    /// `RunCommands`, and so whether `CLOCK_COMMAND` can run. False before
    /// the first answer, and after a grant of a set the bar asked for while
    /// the clock did not need it.
    pub can_run_command: bool,
    /// The hour, counted from the Unix epoch in UTC, `CLOCK_COMMAND` last
    /// ran in, so it runs again in the next one. `None` until it has run,
    /// and again after a new grant.
    pub asked_in_hour: Option<i64>,
    /// When each timer the clock armed and has not yet heard fire is due,
    /// earliest first. Zellij runs each timer on its own, so they fire in
    /// this order, and the one that fires is always the first.
    pub timers_due: Vec<SystemTime>,
    /// The time as last drawn, or `None` while the clock draws nothing.
    pub text: Option<String>,
}

impl Clock {
    /// The offset from UTC to draw the time at, or `None` while the system's
    /// is not known yet.
    fn offset(&self) -> Option<UtcOffset> {
        match self.offset_source {
            OffsetSource::Fixed(offset) => Some(offset),
            OffsetSource::System => self.system_offset,
        }
    }

    /// The time `now` drawn from `format` at the clock's offset, or `None`
    /// while that offset is not known.
    pub(crate) fn text_at(&self, now: SystemTime) -> Option<String> {
        let offset = self.offset()?;
        let (seconds, _) = unix_time(now);
        let time = CivilTime::at(seconds + offset.seconds());
        Some(format_time(&self.format, time, offset))
    }
}

impl State {
    /// Whether the clock is in the tray.
    pub(crate) fn clock_shown(&self) -> bool {
        self.tray.contains(&TrayItem::Clock)
    }

    /// Whether the clock asks the system for the offset from UTC, by
    /// running `CLOCK_COMMAND`, and so needs the `RunCommands` permission.
    pub(crate) fn clock_runs_command(&self) -> bool {
        self.clock_shown() && self.clock.offset_source == OffsetSource::System
    }

    /// Bring the clock up to date with the options and the time, after
    /// anything that may have changed either: arm a timer for the next tick
    /// unless one is due by then, run `CLOCK_COMMAND` if a new hour has
    /// begun since it last ran, and take the time again. Returns whether the
    /// drawn text changed, and so whether the bar should redraw.
    ///
    /// A timer already armed for a later tick, as one for the next minute
    /// when `%S` has just been added to `clock_format`, cannot be canceled,
    /// so a sooner one is armed beside it and the clock shows seconds from
    /// the next. A minute is a whole number of seconds, so the later timer
    /// is due on one of the seconds' ticks too: when the ticks reach it, it
    /// is found due by then and none is armed for that tick, and it stands
    /// for that tick when it fires. So the timers never multiply.
    ///
    /// Only a clock in the tray does any of this. A timer armed before the
    /// clock left the tray still fires, and arms none then.
    pub(crate) fn follow_clock(&mut self) -> bool {
        if !self.clock_shown() {
            return self.clock.text.take().is_some();
        }
        let now = self.host.system_time();
        let delay = Precision::of(&self.clock.format).seconds_until_next_tick(now);
        let next_tick = now + Duration::from_secs_f64(delay);
        // A timer armed for the same tick from an earlier moment may be due a
        // few nanoseconds after this reckoning of it, so the margin counts
        // it as on time.
        let latest_on_time = next_tick + Duration::from_secs_f64(TICK_MARGIN_SECONDS);
        if !self
            .clock
            .timers_due
            .iter()
            .any(|due| *due <= latest_on_time)
        {
            self.host.set_timeout(delay);
            let index = self
                .clock
                .timers_due
                .partition_point(|due| *due <= next_tick);
            self.clock.timers_due.insert(index, next_tick);
        }
        let hour = unix_time(now).0.div_euclid(HOUR);
        if self.clock_runs_command()
            && self.clock.can_run_command
            && self.clock.asked_in_hour != Some(hour)
        {
            let context = BTreeMap::from([(
                CLOCK_CONTEXT_KEY.to_string(),
                CLOCK_CONTEXT_VALUE.to_string(),
            )]);
            self.host.run_command(&CLOCK_COMMAND, context);
            self.clock.asked_in_hour = Some(hour);
        }
        let text = self.clock.text_at(now);
        let changed = text != self.clock.text;
        self.clock.text = text;
        changed
    }

    /// One of the clock's timers fired: the one due first, as Zellij fires
    /// them in the order they are due. The next `follow_clock` takes the
    /// time again and arms the next, unless another is due by then.
    ///
    /// The clock is the only part of the bar that sets a timer, so every
    /// `Event::Timer` the bar gets is one of the clock's.
    pub(crate) fn clock_timer_fired(&mut self) {
        if !self.clock.timers_due.is_empty() {
            self.clock.timers_due.remove(0);
        }
    }

    /// Zellij answered the bar's permission request with `status`, which
    /// `BarCore::update` has taken already: the bar holds its
    /// `granted_permissions`, the whole set it asked for after a grant and
    /// nothing after a denial.
    ///
    /// After a grant that takes in `RunCommands`, the next `follow_clock`
    /// runs `CLOCK_COMMAND` again, as this may be the grant that allowed it.
    /// A grant without it was asked for while the clock needed no command,
    /// with the clock's options changed since, so the clock waits for the
    /// answer to the request those options made. A denial leaves the clock
    /// in UTC unless an offset is already known.
    pub(crate) fn clock_permission_answered(&mut self, status: PermissionStatus) {
        self.clock.can_run_command = self
            .bar
            .granted_permissions
            .contains(&PermissionType::RunCommands);
        if self.clock.can_run_command {
            self.clock.asked_in_hour = None;
        } else if status == PermissionStatus::Denied {
            self.clock.system_offset.get_or_insert(UtcOffset::UTC);
        }
    }

    /// A command the bar ran ended, with `exit_code`, printing `stdout`, and
    /// `context` as the bar sent it. If it is `CLOCK_COMMAND`, take the
    /// offset it printed; one that ended in failure or printed something
    /// else keeps the offset known so far, or UTC while none is. The next
    /// `follow_clock` draws the time at it.
    pub(crate) fn answer_clock_command(
        &mut self,
        exit_code: Option<i32>,
        stdout: &[u8],
        context: &BTreeMap<String, String>,
    ) {
        if context.get(CLOCK_CONTEXT_KEY).map(String::as_str) != Some(CLOCK_CONTEXT_VALUE) {
            return;
        }
        let printed = (exit_code == Some(0))
            .then(|| std::str::from_utf8(stdout).ok())
            .flatten()
            .and_then(UtcOffset::parse);
        match printed {
            Some(offset) => self.clock.system_offset = Some(offset),
            None => {
                self.clock.system_offset.get_or_insert(UtcOffset::UTC);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    /// Whether `year` has a February 29th: one divisible by 4, but not by
    /// 100 unless also by 400. The rule `civil_date` works by, written out
    /// to check it against.
    fn is_leap_year(year: i64) -> bool {
        year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
    }

    /// The Unix time of `year-month-day hour:minute:second` UTC, worked out
    /// by counting days the slow way, to check `CivilTime::at` against.
    fn unix_seconds(year: i64, month: u32, day: u32, hour: u32, minute: u32, second: u32) -> i64 {
        let mut days = 0;
        if year >= 1970 {
            for earlier in 1970..year {
                days += if is_leap_year(earlier) { 366 } else { 365 };
            }
        } else {
            for later in year..1970 {
                days -= if is_leap_year(later) { 366 } else { 365 };
            }
        }
        let month_lengths = [
            31,
            if is_leap_year(year) { 29 } else { 28 },
            31,
            30,
            31,
            30,
            31,
            31,
            30,
            31,
            30,
            31,
        ];
        days += month_lengths[..month as usize - 1].iter().sum::<i64>();
        days += i64::from(day) - 1;
        days * DAY + i64::from(hour) * HOUR + i64::from(minute) * MINUTE + i64::from(second)
    }

    fn at(seconds: i64) -> SystemTime {
        if seconds >= 0 {
            UNIX_EPOCH + Duration::from_secs(seconds as u64)
        } else {
            UNIX_EPOCH - Duration::from_secs(seconds.unsigned_abs())
        }
    }

    #[test]
    fn known_dates_have_their_calendar_day_weekday_and_day_of_year() {
        // (year, month, day, weekday from Sunday, day of the year)
        for (year, month, day, weekday, day_of_year) in [
            (1970, 1, 1, 4, 1),
            (1969, 12, 31, 3, 365),
            (2000, 1, 1, 6, 1),
            // 2000 is divisible by 400, so it is a leap year.
            (2000, 2, 29, 2, 60),
            (2000, 3, 1, 3, 61),
            (2000, 12, 31, 0, 366),
            // 1900 is divisible by 100 and not 400, so it is not.
            (1900, 2, 28, 3, 59),
            (1900, 3, 1, 4, 60),
            (2024, 2, 29, 4, 60),
            (2024, 12, 31, 2, 366),
            (2025, 1, 1, 3, 1),
            (2023, 12, 31, 0, 365),
            (2026, 11, 3, 2, 307),
            (2100, 3, 1, 1, 60),
            (1600, 2, 29, 2, 60),
        ] {
            let time = CivilTime::at(unix_seconds(year, month, day, 12, 0, 0));
            assert_eq!(
                (time.year, time.month, time.day),
                (year, month, day),
                "{year}-{month}-{day}"
            );
            assert_eq!(time.weekday, weekday, "{year}-{month}-{day}");
            assert_eq!(time.day_of_year, day_of_year, "{year}-{month}-{day}");
        }
    }

    #[test]
    fn every_day_over_four_centuries_follows_the_one_before() {
        // From 1899-12-31 to past 2300, through every kind of year, each day
        // is the next date after the last, a weekday on.
        let mut previous = CivilTime::at(unix_seconds(1899, 12, 31, 0, 0, 0));
        let start = unix_seconds(1900, 1, 1, 0, 0, 0) / DAY;
        for days in start..start + 146_097 + 366 {
            let time = CivilTime::at(days * DAY);
            let new_year = time.month == 1 && time.day == 1;
            let new_month = time.day == 1;
            if new_year {
                assert_eq!((time.year, time.day_of_year), (previous.year + 1, 1));
                assert_eq!(
                    previous.day_of_year,
                    if is_leap_year(previous.year) {
                        366
                    } else {
                        365
                    }
                );
            } else if new_month {
                assert_eq!(time.month, previous.month + 1, "{time:?}");
            } else {
                assert_eq!((time.month, time.day), (previous.month, previous.day + 1));
            }
            if !new_year {
                assert_eq!(time.day_of_year, previous.day_of_year + 1, "{time:?}");
            }
            assert_eq!(time.weekday, (previous.weekday + 1) % 7, "{time:?}");
            previous = time;
        }
    }

    #[test]
    fn the_time_of_day_is_split_at_midnight_even_before_the_epoch() {
        let time = CivilTime::at(unix_seconds(2026, 11, 3, 23, 59, 58));
        assert_eq!((time.hour, time.minute, time.second), (23, 59, 58));
        let time = CivilTime::at(-1);
        assert_eq!((time.year, time.month, time.day), (1969, 12, 31));
        assert_eq!((time.hour, time.minute, time.second), (23, 59, 59));
    }

    #[test]
    fn every_specifier_draws_its_part_of_the_time() {
        let time = CivilTime::at(unix_seconds(2024, 3, 5, 14, 7, 9));
        let offset = UtcOffset::parse("-05:30").unwrap();
        for (format, drawn) in [
            ("%H", "14"),
            ("%M", "07"),
            ("%S", "09"),
            ("%I", "02"),
            ("%p", "PM"),
            ("%Y", "2024"),
            ("%y", "24"),
            ("%m", "03"),
            ("%d", "05"),
            ("%e", " 5"),
            ("%j", "065"),
            ("%a", "Tue"),
            ("%A", "Tuesday"),
            ("%b", "Mar"),
            ("%B", "March"),
            ("%z", "-0530"),
            ("%%", "%"),
            ("%H:%M:%S", "14:07:09"),
            ("%a %e %b %Y", "Tue  5 Mar 2024"),
        ] {
            assert_eq!(format_time(format, time, offset), drawn, "{format}");
        }
    }

    #[test]
    fn the_twelve_hour_clock_runs_from_twelve_am_to_eleven_pm() {
        for (hour, drawn) in [
            (0, "12 AM"),
            (1, "01 AM"),
            (11, "11 AM"),
            (12, "12 PM"),
            (13, "01 PM"),
            (23, "11 PM"),
        ] {
            let time = CivilTime::at(unix_seconds(2025, 1, 1, hour, 0, 0));
            assert_eq!(format_time("%I %p", time, UtcOffset::UTC), drawn);
        }
    }

    #[test]
    fn an_unknown_specifier_or_a_lone_percent_is_drawn_as_typed() {
        let time = CivilTime::at(0);
        assert_eq!(format_time("%Q %H", time, UtcOffset::UTC), "%Q 00");
        assert_eq!(format_time("100%", time, UtcOffset::UTC), "100%");
        assert_eq!(format_time("%%H", time, UtcOffset::UTC), "%H");
        assert_eq!(format_time("%é", time, UtcOffset::UTC), "%é");
        assert_eq!(format_time("", time, UtcOffset::UTC), "");
    }

    #[test]
    fn offsets_parse_in_every_written_form_and_nothing_else() {
        for (text, minutes) in [
            ("+02:00", 120),
            ("+0200", 120),
            ("+02", 120),
            ("-0530", -330),
            ("-05:30", -330),
            ("+00:00", 0),
            ("-00", 0),
            ("+23:59", 23 * 60 + 59),
            (" +0545\n", 345),
        ] {
            assert_eq!(
                UtcOffset::parse(text),
                Some(UtcOffset { minutes }),
                "{text:?}"
            );
        }
        for text in [
            "", "0200", "02:00", "+2", "+2:00", "+24:00", "+02:60", "+02:0", "+020", "+02-00",
            "+0200x", "UTC", "+０２", "++0200",
        ] {
            assert_eq!(UtcOffset::parse(text), None, "{text:?}");
        }
    }

    #[test]
    fn an_offset_with_a_character_wider_than_a_byte_is_none() {
        // Each is as many bytes long as an offset is, with a character
        // spanning the byte an offset would be cut at.
        for text in ["+0€", "-0€", "+1é2", "+1é:0", "+€:00", "+é"] {
            assert_eq!(UtcOffset::parse(text), None, "{text:?}");
        }
    }

    #[test]
    fn an_offset_prints_as_date_prints_it() {
        for text in ["+0200", "-0530", "+0000", "+0545"] {
            assert_eq!(UtcOffset::parse(text).unwrap().to_numeric(), text);
        }
    }

    #[test]
    fn an_offset_the_user_wrote_wrong_is_utc_and_an_unset_one_is_the_system_s() {
        assert_eq!(OffsetSource::of(None), OffsetSource::System);
        assert_eq!(
            OffsetSource::of(Some("+01:00")),
            OffsetSource::Fixed(UtcOffset { minutes: 60 })
        );
        assert_eq!(
            OffsetSource::of(Some("Europe/Paris")),
            OffsetSource::Fixed(UtcOffset::UTC)
        );
    }

    #[test]
    fn the_offset_moves_the_clock_across_midnight_and_the_year() {
        let clock = |offset: &str| Clock {
            format: "%Y-%m-%d %H:%M".to_string(),
            offset_source: OffsetSource::of(Some(offset)),
            ..Default::default()
        };
        let new_year_utc = at(unix_seconds(2025, 1, 1, 0, 30, 0));
        assert_eq!(
            clock("+00:00").text_at(new_year_utc).as_deref(),
            Some("2025-01-01 00:30")
        );
        assert_eq!(
            clock("-01:00").text_at(new_year_utc).as_deref(),
            Some("2024-12-31 23:30")
        );
        assert_eq!(
            clock("+05:45").text_at(new_year_utc).as_deref(),
            Some("2025-01-01 06:15")
        );
    }

    #[test]
    fn the_system_s_clock_draws_nothing_until_its_offset_is_known() {
        let mut clock = Clock {
            format: "%H:%M".to_string(),
            ..Default::default()
        };
        assert_eq!(clock.text_at(at(0)), None);
        clock.system_offset = Some(UtcOffset { minutes: 90 });
        assert_eq!(clock.text_at(at(0)).as_deref(), Some("01:30"));
    }

    #[test]
    fn the_clock_ticks_each_second_with_seconds_shown_and_each_minute_otherwise() {
        assert_eq!(Precision::of("%H:%M:%S"), Precision::Second);
        for format in ["%H:%M", "%a %d %b", "", "%%S", "%I %p"] {
            assert_eq!(Precision::of(format), Precision::Minute, "{format}");
        }
    }

    #[test]
    fn the_next_tick_is_just_after_the_next_boundary() {
        let close = |seconds: f64, expected: f64| (seconds - expected).abs() < 1e-6;
        let at_seconds = unix_seconds(2025, 6, 1, 10, 15, 42);
        let now = at(at_seconds) + Duration::from_millis(250);
        assert!(close(
            Precision::Minute.seconds_until_next_tick(now),
            17.75 + TICK_MARGIN_SECONDS
        ));
        assert!(close(
            Precision::Second.seconds_until_next_tick(now),
            0.75 + TICK_MARGIN_SECONDS
        ));
        // Exactly on a boundary, the next is a whole period on.
        let on_minute = at(unix_seconds(2025, 6, 1, 10, 16, 0));
        assert!(close(
            Precision::Minute.seconds_until_next_tick(on_minute),
            60.0 + TICK_MARGIN_SECONDS
        ));
        // Before the epoch too.
        let before = at(-30);
        assert!(close(
            Precision::Minute.seconds_until_next_tick(before),
            30.0 + TICK_MARGIN_SECONDS
        ));
    }

    #[test]
    fn a_time_before_the_epoch_counts_down_from_it() {
        assert_eq!(unix_time(at(-30)), (-30, 0));
        assert_eq!(
            unix_time(UNIX_EPOCH - Duration::from_millis(1500)),
            (-2, 500_000_000)
        );
        assert_eq!(unix_time(at(90)), (90, 0));
    }
}
