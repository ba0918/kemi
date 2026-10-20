//! セッション id の ULID（R-SESSION）。48 bit の時刻 + 80 bit の乱数を 26 文字にする。

use std::hash::{BuildHasher, Hasher};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

/// Crockford の base32。並びは ULID の仕様のまま。
const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

/// このモジュールが作る 26 文字の ULID かどうか。セッションの id として受け取る
/// 文字列の検証に使う（R-SESSION）。
pub fn is_valid_id(id: &str) -> bool {
    id.len() == 26 && id.bytes().all(|byte| ALPHABET.contains(&byte))
}

/// 26 文字の ULID。128 bit を 5 bit ずつ、上位から並べる。
pub(crate) fn generate_ulid(time_millis: u128, random: [u8; 10]) -> String {
    let mut value = time_millis & 0xffff_ffff_ffff;
    for byte in random {
        value = (value << 8) | u128::from(byte);
    }
    let mut out = [0u8; 26];
    for index in (0..26).rev() {
        out[index] = ALPHABET[(value & 31) as usize];
        value >>= 5;
    }
    String::from_utf8(out.to_vec()).expect("the alphabet is ASCII")
}

/// 時刻と乱数から新しい ULID を作る。
pub fn new_ulid() -> String {
    static LAST: Mutex<Option<(u128, [u8; 10])>> = Mutex::new(None);
    let now = now_millis();
    let mut last = LAST.lock().expect("ulid lock poisoned");
    let (time, random) = next_parts(*last, now, || random_bytes(now));
    *last = Some((time, random));
    generate_ulid(time, random)
}

/// 直前に作った ULID の（時刻, 乱数）から次の組を決める。時刻が進まない間（同じミリ秒、
/// 時計の後退）は直前の時刻のまま乱数側を 1 進め、同じミリ秒でも衝突せず、作った順に
/// 並ぶようにする。時刻が進んだら `fresh` の乱数で始め直す。
fn next_parts(
    last: Option<(u128, [u8; 10])>,
    now: u128,
    fresh: impl FnOnce() -> [u8; 10],
) -> (u128, [u8; 10]) {
    match last {
        Some((time, mut random)) if now <= time => {
            increment(&mut random);
            (time, random)
        }
        Some(_) | None => (now, fresh()),
    }
}

/// 現在時刻のミリ秒。時計が戻っても 0 で止める。
pub fn now_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0)
}

/// プロセス内で一度でも使えば十分な 80 bit。OS の乱数で種を入れたハッシュを使う。
fn random_bytes(now: u128) -> [u8; 10] {
    let mut out = [0u8; 10];
    for chunk in 0..2 {
        let state = std::collections::hash_map::RandomState::new();
        let mut hasher = state.build_hasher();
        hasher.write_u32(std::process::id());
        hasher.write_usize(&state as *const _ as usize);
        hasher.write_u128(now);
        let value = hasher.finish().to_le_bytes();
        let count = out.len().saturating_sub(chunk * 8).min(8);
        out[chunk * 8..chunk * 8 + count].copy_from_slice(&value[..count]);
    }
    out
}

/// 80 bit を 1 増やす。桁あふれは 0 に戻す。
fn increment(random: &mut [u8; 10]) {
    for byte in random.iter_mut().rev() {
        *byte = byte.wrapping_add(1);
        if *byte != 0 {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn ulid_is_26_chars_of_crockford_base32() {
        let id = generate_ulid(1_700_000_000_000, [0; 10]);

        assert_eq!(id.len(), 26);
        assert!(id.bytes().all(|byte| ALPHABET.contains(&byte)));
    }

    #[test]
    fn ulid_starts_with_the_time() {
        let id = generate_ulid(1_700_000_000_000, [0; 10]);

        assert_eq!(&id[..10], "01HF7YAT00");
    }

    fn no_fresh_random() -> [u8; 10] {
        panic!("the time did not advance, so the random part must continue")
    }

    #[test]
    fn ulid_in_the_same_millisecond_keeps_the_time_and_increments_the_random_part() {
        let last = (1_000, [7, 0, 0, 0, 0, 0, 0, 0, 0, 1]);

        let next = next_parts(Some(last), 1_000, no_fresh_random);

        assert_eq!(next, (1_000, [7, 0, 0, 0, 0, 0, 0, 0, 0, 2]));
        assert!(generate_ulid(next.0, next.1) > generate_ulid(last.0, last.1));
    }

    #[test]
    fn ulid_after_the_clock_goes_back_keeps_the_previous_time_and_order() {
        let last = (1_000, [7, 0, 0, 0, 0, 0, 0, 0, 0, 1]);

        let next = next_parts(Some(last), 999, no_fresh_random);

        assert_eq!(next, (1_000, [7, 0, 0, 0, 0, 0, 0, 0, 0, 2]));
        assert!(generate_ulid(next.0, next.1) > generate_ulid(last.0, last.1));
    }

    #[test]
    fn ulid_starts_a_fresh_random_part_when_the_time_advances_or_on_the_first_call() {
        let fresh = [9; 10];

        assert_eq!(
            next_parts(Some((1_000, [7; 10])), 1_001, || fresh),
            (1_001, fresh)
        );
        assert_eq!(next_parts(None, 1_000, || fresh), (1_000, fresh));
    }

    #[test]
    fn ulid_random_part_carries_into_the_higher_bytes() {
        let next = next_parts(
            Some((1_000, [0, 0, 0, 0, 0, 0, 0, 1, 0xff, 0xff])),
            1_000,
            no_fresh_random,
        );

        assert_eq!(next, (1_000, [0, 0, 0, 0, 0, 0, 0, 2, 0, 0]));
    }

    #[test]
    fn ulid_is_unique_across_many_calls() {
        let mut ids = HashSet::new();
        for _ in 0..1000 {
            ids.insert(new_ulid());
        }

        assert_eq!(ids.len(), 1000);
    }

    #[test]
    fn ulid_keeps_the_order_of_consecutive_calls() {
        let mut previous = new_ulid();
        for _ in 0..10 {
            let next = new_ulid();
            assert!(next > previous, "{next} must sort after {previous}");
            previous = next;
        }
    }
}
