//! LAN 公開の共有アドレス特定と警告文（R-SERVE）。

use std::net::{Ipv4Addr, SocketAddr, UdpSocket};

/// 既定経路のローカルアドレス＝共有アドレス（R-SERVE）。宛先へはパケットを送らず、
/// 経路の選択だけをさせる。特定できなければ None。
pub fn detect_share_address() -> Option<Ipv4Addr> {
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).ok()?;
    // TEST-NET-1（RFC 5737）。送信しないので、宛先の到達性は要らない。
    socket.connect((Ipv4Addr::new(192, 0, 2, 1), 80)).ok()?;
    match socket.local_addr().ok()? {
        SocketAddr::V4(address) if !address.ip().is_unspecified() => Some(*address.ip()),
        _ => None,
    }
}

/// 共有アドレスを特定できたときの固定の 1 文（R-SERVE）。
const FIXED_WARNING: &str =
    "kemi: exposed on the LAN; anyone with the URL can read the diff and submit over plain HTTP.";

/// wildcard で共有アドレスを特定できなかったときの警告行（R-SERVE）。取得できなかった
/// ことと `--bind` の案内、公開のリスクを述べる。
const FALLBACK_WARNING: &str = "kemi: exposed on the LAN; could not determine this machine's \
    address, so the URL uses 127.0.0.1. Anyone who can reach this port can read the diff and \
    submit over plain HTTP; pass --bind <your-ip> to print a reachable URL.";

/// 非ループバックで待つときの警告行（R-SERVE）。ループバックなら None。
pub fn exposure_warning(bind: Ipv4Addr, share_address: Option<Ipv4Addr>) -> Option<String> {
    if bind.is_loopback() {
        return None;
    }
    let text = if bind.is_unspecified() && share_address.is_none() {
        FALLBACK_WARNING
    } else {
        FIXED_WARNING
    };
    Some(text.to_string())
}

#[cfg(test)]
mod tests {
    use std::net::Ipv4Addr;

    use super::*;

    const FIXED: &str =
        "kemi: exposed on the LAN; anyone with the URL can read the diff and submit over plain HTTP.";

    #[test]
    fn exposure_warning_is_none_for_loopback() {
        assert_eq!(exposure_warning(Ipv4Addr::LOCALHOST, None), None);
        assert_eq!(
            exposure_warning(
                Ipv4Addr::new(127, 1, 2, 3),
                Some(Ipv4Addr::new(192, 168, 1, 5))
            ),
            None
        );
    }

    #[test]
    fn exposure_warning_is_fixed_for_a_concrete_address() {
        assert_eq!(
            exposure_warning(Ipv4Addr::new(192, 168, 1, 5), None).as_deref(),
            Some(FIXED)
        );
    }

    #[test]
    fn exposure_warning_is_fixed_for_a_wildcard_with_a_share_address() {
        assert_eq!(
            exposure_warning(Ipv4Addr::UNSPECIFIED, Some(Ipv4Addr::new(192, 168, 1, 5))).as_deref(),
            Some(FIXED)
        );
    }

    #[test]
    fn exposure_warning_falls_back_when_a_wildcard_has_no_share_address() {
        let warning = exposure_warning(Ipv4Addr::UNSPECIFIED, None).expect("a wildcard must warn");

        assert!(
            warning.starts_with("kemi: exposed on the LAN;"),
            "{warning}"
        );
        assert!(warning.contains("--bind"), "{warning}");
        assert!(warning.contains("read the diff"), "{warning}");
        assert!(warning.contains("submit"), "{warning}");
        assert!(warning.contains("plain HTTP"), "{warning}");
    }
}
