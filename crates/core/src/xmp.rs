pub const XMP_SCAN_LIMIT: u64 = 4 * 1024 * 1024;
const PACKET_START: &[u8] = b"<x:xmpmeta";
const PACKET_END: &[u8] = b"</x:xmpmeta>";
const MAX_PACKET_LEN: usize = 1024 * 1024;

fn find(haystack: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    haystack
        .get(from..)?
        .windows(needle.len())
        .position(|window| window == needle)
        .map(|position| position + from)
}

pub fn embedded_packet(bytes: &[u8]) -> Option<&str> {
    let start = find(bytes, PACKET_START, 0)?;
    let search_end = bytes.len().min(start + MAX_PACKET_LEN);
    let end = find(bytes.get(..search_end)?, PACKET_END, start)? + PACKET_END.len();
    std::str::from_utf8(bytes.get(start..end)?).ok()
}

pub fn property<'a>(packet: &'a str, name: &str) -> Option<&'a str> {
    let attribute = format!("{name}=");
    if let Some(position) = packet.find(&attribute) {
        let rest = &packet[position + attribute.len()..];
        let quote = rest.chars().next().filter(|c| *c == '"' || *c == '\'')?;
        let value = &rest[1..];
        return value.find(quote).map(|end| &value[..end]);
    }
    let open = format!("<{name}>");
    let position = packet.find(&open)?;
    let value = &packet[position + open.len()..];
    value.find('<').map(|end| value[..end].trim())
}

pub fn rating(bytes: &[u8]) -> Option<u8> {
    let packet = embedded_packet(bytes)?;
    property(packet, "xmp:Rating")?
        .trim()
        .parse::<i8>()
        .ok()
        .and_then(|value| u8::try_from(value).ok())
        .filter(|value| (1..=5).contains(value))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wrap(packet: &str) -> Vec<u8> {
        let mut bytes = vec![0xFF, 0xD8, 0, 1, 2];
        bytes.extend_from_slice(packet.as_bytes());
        bytes.extend_from_slice(&[0xFF, 0xD9]);
        bytes
    }

    #[test]
    fn reads_rating_attribute() {
        let bytes = wrap(
            r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:Description xmp:Rating="4"/></x:xmpmeta>"#,
        );
        assert_eq!(rating(&bytes), Some(4));
    }

    #[test]
    fn reads_rating_element() {
        let bytes = wrap(
            "<x:xmpmeta><rdf:Description><xmp:Rating> 3 </xmp:Rating></rdf:Description></x:xmpmeta>",
        );
        assert_eq!(rating(&bytes), Some(3));
    }

    #[test]
    fn ignores_reject_and_out_of_range_ratings() {
        assert_eq!(
            rating(&wrap(r#"<x:xmpmeta xmp:Rating="-1"></x:xmpmeta>"#)),
            None
        );
        assert_eq!(
            rating(&wrap(r#"<x:xmpmeta xmp:Rating="0"></x:xmpmeta>"#)),
            None
        );
        assert_eq!(
            rating(&wrap(r#"<x:xmpmeta xmp:Rating="9"></x:xmpmeta>"#)),
            None
        );
    }

    #[test]
    fn missing_or_unterminated_packet_has_no_rating() {
        assert_eq!(rating(b"no xmp here"), None);
        assert_eq!(rating(&wrap(r#"<x:xmpmeta xmp:Rating="4">"#)), None);
    }
}
