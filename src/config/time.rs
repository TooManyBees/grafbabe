// transliterated from musl __secs_to_tm.c
// http://git.musl-libc.org/cgit/musl/tree/src/time/__secs_to_tm.c

use std::fmt;
use std::time::{SystemTime, UNIX_EPOCH};

const SECS_PER_DAY: i64 = 24 * 60 * 60;
const LEAPOCH: i64 = 946684800 + SECS_PER_DAY * (31 + 29);
const DAYS_PER_400Y: i64 = 365 * 400 + 97;
const DAYS_PER_100Y: i64 = 365 * 100 + 24;
const DAYS_PER_4Y: i64 = 365 * 4 + 1;

pub struct Time {
    year: i64,
    month: u8,
    mday: u8,
    hour: u8,
    minute: u8,
    second: u8,
    nanos: u32,
}

impl fmt::Display for Time {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
            self.year,
            self.month,
            self.mday,
            self.hour,
            self.minute,
            self.second,
            self.nanos / 1000000,
        )
    }
}

impl Time {
    pub fn from(now: SystemTime) -> Self {
        const DAYS_IN_MONTH: [i64; 12] = [31, 30, 31, 30, 31, 31, 30, 31, 30, 31, 31, 29];

        /*
        if (t < INT_MIN * 31622400LL || t > INT_MAX * 31622400LL)
            return -1;
        */

        let (t, nanos) = match now.duration_since(UNIX_EPOCH) {
            Ok(duration) => (duration.as_secs() as i64, duration.subsec_nanos()),
            Err(err) => {
                let t = err.duration().as_secs() as i64;
                let nanos = err.duration().subsec_nanos();
                if nanos > 0 {
                    (-t - 1, 1000000000 - nanos)
                } else {
                    (-t, 0)
                }
            }
        };

        let secs = t - LEAPOCH;
        let (days, remsecs) = {
            let mut days = secs / SECS_PER_DAY;
            let mut remsecs = secs % SECS_PER_DAY;

            if remsecs < 0 {
                remsecs += SECS_PER_DAY;
                days -= 1;
            }

            (days, remsecs)
        };

        let (qc_cycles, mut remdays) = {
            let mut qc_cycles = days / DAYS_PER_400Y;
            let mut remdays = days % DAYS_PER_400Y;

            if remdays < 0 {
                remdays += DAYS_PER_400Y;
                qc_cycles -= 1;
            }

            (qc_cycles, remdays)
        };

        let c_cycles = {
            let mut c_cycles = remdays / DAYS_PER_100Y;
            if c_cycles == 4 {
                c_cycles -= 1;
            }
            remdays -= c_cycles * DAYS_PER_100Y;

            c_cycles
        };

        let q_cycles = {
            let mut q_cycles = remdays / DAYS_PER_4Y;
            if q_cycles == 25 {
                q_cycles -= 1;
            }
            remdays -= q_cycles * DAYS_PER_4Y;

            q_cycles
        };

        let remyears = {
            let mut remyears = remdays / 365;
            if remyears == 4 {
                remyears -= 1;
            }
            remdays -= remyears * 365;

            remyears
        };

        let mut years = remyears + 4 * q_cycles + 100 * c_cycles + 400 * qc_cycles;

        let mut months: i8 = 0;
        for (m, days_in_month) in DAYS_IN_MONTH.iter().copied().enumerate() {
            months = m as i8;
            if days_in_month > remdays {
                break;
            }
            remdays -= days_in_month;
        }

        if months >= 10 {
            months -= 12;
            years += 1;
        }

        Time {
            year: years + 2000,
            month: (months + 2) as u8,
            mday: (remdays + 1) as u8,
            hour: (remsecs / 3600) as u8,
            minute: (remsecs / 60 % 60) as u8,
            second: (remsecs % 60) as u8,
            nanos: nanos,
        }
    }
}
