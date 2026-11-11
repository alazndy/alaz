//! Fixed global clock. No speed-up: work speed scales with tech/automation instead.

pub const TICKS_PER_SECOND: u64 = 60;
/// One in-game day = 24 real minutes.
pub const DAY_SECONDS: u64 = 24 * 60;
pub const TICKS_PER_DAY: u64 = DAY_SECONDS * TICKS_PER_SECOND;
/// The world starts at 08:00.
const START_MINUTE: u64 = 8 * 60;

/// In-game minute of the day (0..1440).
pub fn minute_of_day(tick: u64) -> u32 {
    (((tick % TICKS_PER_DAY) * 1440 / TICKS_PER_DAY + START_MINUTE) % 1440) as u32
}

/// Days in one season in worlds made before the season length became a world setting; they keep it.
pub const SEASON_DAYS: u64 = 3;
/// Days in one season in a new world (a world setting): the year is four of them, spring, summer, autumn, winter.
pub const NEW_SEASON_DAYS: u64 = 6;

/// 0 spring, 1 summer, 2 autumn, 3 winter, for seasons of `days` days. The world starts in spring.
pub fn season(tick: u64, days: u64) -> u8 {
    ((tick / TICKS_PER_DAY / days.max(1)) % 4) as u8
}

/// How far the season shifts the temperature, tenths of a degree.
pub fn season_shift_x10(tick: u64, days: u64) -> i32 {
    [0, 60, -30, -130][season(tick, days) as usize]
}

/// Ambient temperature in tenths of a degree C. By day: 0.0 at 03:00, 21.0 at 15:00, then the season's shift.
pub fn ambient_x10(tick: u64, days: u64) -> i32 {
    let m = minute_of_day(tick) as i32;
    let d = (m - 900).abs();
    let d = d.min(1440 - d);
    210 * (720 - d) / 720 + season_shift_x10(tick, days)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Weather {
    Clear,
    Rain,
    Snow,
}

/// Weather in blocks of four in-game hours, drawn from the world seed so every peer sees the same sky.
/// Spring and autumn are wet, summer mostly dry, winter brings snow. The first day is always clear.
pub fn weather(seed: u64, tick: u64, days: u64) -> Weather {
    if tick < TICKS_PER_DAY {
        return Weather::Clear;
    }
    let block = (tick / (TICKS_PER_DAY / 6)) as i32;
    let roll = crate::rng::hash2(seed ^ 0x7EA7, block, 0) % 100;
    let chance = [28, 10, 38, 34][season(tick, days) as usize];
    if roll >= chance {
        Weather::Clear
    } else if season(tick, days) == 3 {
        Weather::Snow
    } else {
        Weather::Rain
    }
}

/// Light level 300..=1000 (permille): full 08:00-17:00, dim 20:00-05:00, linear ramps between.
pub fn daylight_permille(tick: u64) -> i32 {
    let m = minute_of_day(tick) as i32;
    match m {
        480..=1019 => 1000,
        1020..=1199 => 1000 - 700 * (m - 1020) / 180,
        300..=479 => 300 + 700 * (m - 300) / 180,
        _ => 300,
    }
}

/// The wind over the whole world at a tick: (heading the air moves toward, in milliradians 0..6283, 0 = +x;
/// strength in permille). The heading wanders slowly over the day, the strength rises with bad weather and gusts
/// come and go. Pure: every peer, and the renderer, sees the same wind.
pub fn wind(seed: u64, tick: u64, days: u64) -> (i32, i32) {
    let smooth = |salt: u64, period: u64, range: i64| -> i64 {
        let (b, f) = ((tick / period) as i32, (tick % period) as i64 * 1000 / period as i64);
        let s = f * f * (3000 - 2 * f) / 1_000_000; // smoothstep, permille
        let a = (crate::rng::hash2(seed ^ salt, b, 0) % range as u64) as i64;
        let c = (crate::rng::hash2(seed ^ salt, b + 1, 0) % range as u64) as i64;
        a * (1000 - s) / 1000 + c * s / 1000
    };
    // heading: blend the shorter way round between two directions a quarter-day apart
    let period = TICKS_PER_DAY / 4;
    let (b, f) = ((tick / period) as i32, (tick % period) as i64 * 1000 / period as i64);
    let s = f * f * (3000 - 2 * f) / 1_000_000;
    let a0 = (crate::rng::hash2(seed ^ 0x3D11, b, 0) % 6283) as i64;
    let a1 = (crate::rng::hash2(seed ^ 0x3D11, b + 1, 0) % 6283) as i64;
    let mut d = a1 - a0;
    if d > 3141 {
        d -= 6283;
    } else if d < -3141 {
        d += 6283;
    }
    let heading = (a0 + d * s / 1000).rem_euclid(6283) as i32;
    let calm = smooth(0x3D12, TICKS_PER_DAY / 6, 450);
    let gust = smooth(0x3D13, 300, 200);
    // a storm's wind rises (and falls) over half a minute when the weather turns, not at once
    let block = TICKS_PER_DAY / 6;
    let into = tick % block;
    let stormy = |t: u64| if weather(seed, t, days) != Weather::Clear { 380 } else { 0 };
    let now = stormy(tick);
    let before = if tick >= into + 1 { stormy(tick - into - 1) } else { now };
    let ramp = (into as i64 * 1000 / 1800).min(1000);
    let storm = before + (now - before) * ramp / 1000;
    (heading, (120 + calm + gust + storm).clamp(0, 1000) as i32)
}
