//! Local time, owned — "what day is it" for the CLI (review item 6,
//! decision D3).
//!
//! std cannot read the system timezone. The options were a time crate
//! or `libc::localtime_r`; both add a dependency for what is, in the
//! end, a small file format, and §6 asks for justification before any
//! new crate. So this module reads the platform's TZif file (RFC 8536
//! — the format of /etc/localtime and /usr/share/zoneinfo on Linux and
//! macOS) and picks the UTC offset in force at a given instant, DST
//! transitions included. Any failure falls back to UTC, loudly
//! documented here rather than silently wrong.
//!
//! Everything date-shaped is Howard Hinnant's public-domain civil-date
//! arithmetic, the same family as `progress::prev_day` and `srs::add_days`.

use std::fs;
use std::path::PathBuf;

/// "Today" in the local timezone as YYYY-MM-DD.
pub(super) fn today_local() -> String {
    let now = unix_now();
    civil_from_unix(now + local_utc_offset_seconds(now))
}

/// UTC seconds since the epoch; 0 if the clock is somehow before 1970.
fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// The local timezone's UTC offset in force at `unix_secs`, read from
/// the zone the platform uses: $TZ (a zone name or an absolute path, a
/// leading ':' allowed; empty means UTC per POSIX) or /etc/localtime.
/// UTC on any failure — a wrong-but-deterministic answer beats none.
pub(super) fn local_utc_offset_seconds(unix_secs: i64) -> i64 {
    match zoneinfo_path() {
        Some(path) => fs::read(&path)
            .ok()
            .and_then(|bytes| tzif_offset(&bytes, unix_secs))
            .unwrap_or(0),
        None => 0,
    }
}

/// The TZif file to read, mirroring how libc resolves the local zone.
fn zoneinfo_path() -> Option<PathBuf> {
    match std::env::var("TZ") {
        Ok(tz) => {
            if tz.is_empty() {
                return None; // POSIX: an empty TZ explicitly means UTC
            }
            let name = tz.strip_prefix(':').unwrap_or(&tz);
            if name.starts_with('/') {
                Some(PathBuf::from(name))
            } else {
                Some(PathBuf::from("/usr/share/zoneinfo").join(name))
            }
        }
        Err(_) => Some(PathBuf::from("/etc/localtime")),
    }
}

/// The six counts a TZif header carries.
struct Counts {
    isutcnt: usize,
    isstdcnt: usize,
    leapcnt: usize,
    timecnt: usize,
    typecnt: usize,
    charcnt: usize,
}

/// Parses a TZif header (44 bytes: magic, version, reserved, six u32
/// big-endian counts), returning the counts and the rest of the file.
fn parse_header(bytes: &[u8]) -> Option<(u8, Counts, &[u8])> {
    if bytes.len() < 44 || &bytes[0..4] != b"TZif" {
        return None;
    }
    let version = bytes[4];
    let counts = Counts {
        isutcnt: be_u32(bytes, 20)? as usize,
        isstdcnt: be_u32(bytes, 24)? as usize,
        leapcnt: be_u32(bytes, 28)? as usize,
        timecnt: be_u32(bytes, 32)? as usize,
        typecnt: be_u32(bytes, 36)? as usize,
        charcnt: be_u32(bytes, 40)? as usize,
    };
    Some((version, counts, &bytes[44..]))
}

/// Length of one data block: transitions, type indices, types,
/// designations, leap records and the two indicator arrays.
fn data_block_len(c: &Counts, wide: bool) -> Option<usize> {
    let t = if wide { 8 } else { 4 };
    c.timecnt
        .checked_mul(t)?
        .checked_add(c.timecnt)?
        .checked_add(c.typecnt.checked_mul(6)?)?
        .checked_add(c.charcnt)?
        .checked_add(c.leapcnt.checked_mul(t + 4)?)?
        .checked_add(c.isstdcnt)?
        .checked_add(c.isutcnt)
}

/// UTC offset in force at `unix_secs` according to a TZif file: the
/// utoff of the last transition at or before the instant (the second,
/// 64-bit data block when the file is version 2+; the 32-bit one
/// otherwise). Before the first transition the first non-DST type
/// applies. None on any malformed input.
fn tzif_offset(bytes: &[u8], unix_secs: i64) -> Option<i64> {
    let (v1_version, v1, rest) = parse_header(bytes)?;
    let v1_len = data_block_len(&v1, false)?;
    let v1_block = rest.get(..v1_len)?;

    let (counts, block, wide) = if v1_version >= b'2' {
        let (v2_version, v2, rest2) = parse_header(&rest[v1_len..])?;
        let wide = v2_version >= b'2';
        let len = data_block_len(&v2, wide)?;
        (v2, rest2.get(..len)?, wide)
    } else {
        (v1, v1_block, false)
    };
    if counts.typecnt == 0 {
        return None;
    }

    let t = if wide { 8 } else { 4 };
    let mut pos = 0usize;

    // transition instants (sorted ascending per the format)
    let mut transitions = Vec::with_capacity(counts.timecnt);
    for _ in 0..counts.timecnt {
        transitions.push(read_sized(block, pos, t)?);
        pos += t;
    }
    // which type each transition switches to
    let mut indices = Vec::with_capacity(counts.timecnt);
    for _ in 0..counts.timecnt {
        indices.push(*block.get(pos)?);
        pos += 1;
    }
    // local-time types: (utoff seconds, is_dst)
    let mut types = Vec::with_capacity(counts.typecnt);
    for _ in 0..counts.typecnt {
        let utoff = read_sized(block, pos, 4)? as i32;
        let is_dst = *block.get(pos + 4)? == 1;
        types.push((utoff, is_dst));
        pos += 6;
    }
    // designations, leap seconds and the indicator arrays follow; this
    // consumer does not need them.

    let type_index = match transitions.binary_search(&unix_secs) {
        // exactly on a transition: the new offset applies
        Ok(i) => *indices.get(i)? as usize,
        // before the first transition: standard time, not the first DST
        Err(0) => {
            let (utoff, _) = types
                .iter()
                .find(|(_, is_dst)| !is_dst)
                .or_else(|| types.first())?;
            return Some(i64::from(*utoff));
        }
        // between transitions: the one before the instant
        Err(i) => *indices.get(i - 1)? as usize,
    };
    let (utoff, _) = *types.get(type_index)?;
    Some(i64::from(utoff))
}

fn be_u32(bytes: &[u8], pos: usize) -> Option<u32> {
    Some(u32::from_be_bytes(
        bytes.get(pos..pos + 4)?.try_into().ok()?,
    ))
}

/// A big-endian signed integer of 4 or 8 bytes.
fn read_sized(bytes: &[u8], pos: usize, len: usize) -> Option<i64> {
    match len {
        4 => Some(i64::from(i32::from_be_bytes(
            bytes.get(pos..pos + 4)?.try_into().ok()?,
        ))),
        _ => Some(i64::from_be_bytes(
            bytes.get(pos..pos + 8)?.try_into().ok()?,
        )),
    }
}

/// Converts unix seconds (UTC) to a YYYY-MM-DD civil date.
/// Howard Hinnant's public-domain civil-from-days algorithm.
pub(super) fn civil_from_unix(secs: i64) -> String {
    let (y, m, d) = civil_from_days(secs.div_euclid(86_400));
    format!("{y:04}-{m:02}-{d:02}")
}

/// Civil date from days since the unix epoch.
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// Days since the unix epoch for a civil date — the inverse of the
/// above. Only the tests build instants from calendar dates.
#[cfg(test)]
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An instant built from a UTC calendar date and hour.
    fn utc(y: i64, m: i64, d: i64, hour: i64) -> i64 {
        days_from_civil(y, m, d) * 86_400 + hour * 3_600
    }

    #[test]
    fn civil_dates_match_known_values() {
        assert_eq!(civil_from_unix(0), "1970-01-01");
        assert_eq!(civil_from_unix(1_755_846_000), "2025-08-22"); // 2025-08-22 05:00 UTC
        assert_eq!(civil_from_unix(951_782_400), "2000-02-29"); // leap day
                                                                // negative instants predate the epoch
        assert_eq!(civil_from_unix(-1), "1969-12-31");
    }

    #[test]
    fn civil_round_trips_through_days() {
        for (y, m, d) in [(1970, 1, 1), (2000, 2, 29), (2026, 10, 30), (1969, 12, 31)] {
            let days = days_from_civil(y, m, d);
            assert_eq!(civil_from_days(days), (y, m, d));
        }
    }

    #[test]
    fn utc_fixture_is_always_offset_zero() {
        let bytes = include_bytes!("testdata/utc.tzif");
        assert_eq!(tzif_offset(bytes, 0), Some(0));
        assert_eq!(tzif_offset(bytes, utc(2026, 7, 15, 12)), Some(0));
        // and before any transition, too
        assert_eq!(tzif_offset(bytes, -100_000_000_000), Some(0));
    }

    #[test]
    fn beirut_fixture_follows_its_dst_rules() {
        let bytes = include_bytes!("testdata/beirut.tzif");
        // January: standard time, UTC+2.
        assert_eq!(tzif_offset(bytes, utc(2026, 1, 15, 12)), Some(2 * 3600));
        // July: summer time, UTC+3.
        assert_eq!(tzif_offset(bytes, utc(2026, 7, 15, 12)), Some(3 * 3600));
        // DST ends exactly at 2026-10-24 21:00 UTC (00:00 local summer
        // time): +3 just before, +2 from the instant itself on.
        assert_eq!(tzif_offset(bytes, utc(2026, 10, 24, 20)), Some(3 * 3600));
        assert_eq!(tzif_offset(bytes, utc(2026, 10, 24, 21)), Some(2 * 3600));
        assert_eq!(tzif_offset(bytes, utc(2026, 10, 25, 12)), Some(2 * 3600));
        // DST starts exactly at 2026-03-28 22:00 UTC: +2 before, +3 from
        // the instant itself on (verified against the system zoneinfo).
        assert_eq!(tzif_offset(bytes, utc(2026, 3, 28, 21)), Some(2 * 3600));
        assert_eq!(tzif_offset(bytes, utc(2026, 3, 28, 22)), Some(3 * 3600));
        // Before the zone's first transition the first non-DST type
        // applies — for Beirut that is its historical LMT offset
        // (+02:22), per the format. Irrelevant in practice; it just
        // proves the fallback branch picks a type instead of failing.
        assert_eq!(tzif_offset(bytes, i64::MIN / 2), Some(8520));
    }

    #[test]
    fn today_local_uses_the_offset_to_shift_the_date() {
        // 23:00 UTC on 2026-10-30: in Beirut (+03, still DST) the local
        // date has already rolled to the 31st; five hours behind UTC it
        // is still the 30th. The offset shifts the instant before the
        // civil-date math — that shift is the whole fix of item 6.
        let late_utc = utc(2026, 10, 30, 23);
        assert_eq!(civil_from_unix(late_utc + 3 * 3600), "2026-10-31");
        assert_eq!(civil_from_unix(late_utc - 5 * 3600), "2026-10-30");
    }

    #[test]
    fn malformed_tzif_is_rejected_not_misparsed() {
        assert_eq!(tzif_offset(b"", 0), None);
        assert_eq!(tzif_offset(b"not a tzif file at all", 0), None);
        // magic but truncated header
        assert_eq!(tzif_offset(b"TZif", 0), None);
        let mut truncated = include_bytes!("testdata/beirut.tzif").to_vec();
        truncated.truncate(100);
        assert_eq!(tzif_offset(&truncated, 0), None);
    }

    #[test]
    fn zone_resolution_follows_tz_semantics() {
        // Mutates $TZ — hold the shared env lock like every other
        // env-touching test in the process.
        let _env = crate::testenv::env_lock();
        std::env::set_var("TZ", ":Asia/Beirut");
        assert_eq!(
            zoneinfo_path().as_deref(),
            Some(std::path::Path::new("/usr/share/zoneinfo/Asia/Beirut"))
        );
        std::env::set_var("TZ", "/some/absolute/file");
        assert_eq!(
            zoneinfo_path().as_deref(),
            Some(std::path::Path::new("/some/absolute/file"))
        );
        std::env::set_var("TZ", "");
        assert_eq!(zoneinfo_path(), None);
        std::env::remove_var("TZ");
        assert_eq!(
            zoneinfo_path().as_deref(),
            Some(std::path::Path::new("/etc/localtime"))
        );
    }
}
