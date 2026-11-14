pub type Checker = fn(&str) -> Vec<usize>;

pub fn checker_for(extension: &str) -> Option<Checker> {
    match extension {
        "rs" => Some(rust),
        "svelte" => Some(svelte),
        "css" => Some(css),
        _ => None,
    }
}

fn line_at(text: &[u8], index: usize) -> usize {
    text[..index.min(text.len())]
        .iter()
        .filter(|&&byte| byte == b'\n')
        .count()
        + 1
}

fn find(text: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    text.get(from..)?
        .windows(needle.len())
        .position(|window| window == needle)
        .map(|position| position + from)
}

fn is_ident(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn raw_string_end(text: &[u8], start: usize) -> Option<usize> {
    let mut index = start;
    if matches!(text.get(index), Some(b'b' | b'c')) {
        index += 1;
    }
    if text.get(index) != Some(&b'r') {
        return None;
    }
    index += 1;
    let hashes = text[index..]
        .iter()
        .take_while(|&&byte| byte == b'#')
        .count();
    index += hashes;
    if text.get(index) != Some(&b'"') {
        return None;
    }
    let mut closing = vec![b'"'];
    closing.extend(std::iter::repeat_n(b'#', hashes));
    Some(find(text, &closing, index + 1).map_or(text.len(), |end| end + closing.len()))
}

fn char_literal_end(source: &str, start: usize) -> Option<usize> {
    let text = source.as_bytes();
    let rest = text.get(start + 1..)?;
    match rest.first()? {
        b'\\' => {
            let close = rest.get(2..)?.iter().position(|&byte| byte == b'\'')?;
            (close <= 8).then_some(start + close + 4)
        }
        b'\'' => None,
        _ => {
            let width = source
                .get(start + 1..)?
                .chars()
                .next()
                .map_or(1, char::len_utf8);
            (rest.get(width) == Some(&b'\'')).then_some(start + width + 2)
        }
    }
}

pub fn rust(source: &str) -> Vec<usize> {
    let text = source.as_bytes();
    let mut found = Vec::new();
    let mut index = 0;
    while index < text.len() {
        let rest = &text[index..];
        if rest.starts_with(b"//") {
            found.push(line_at(text, index));
            index = find(text, b"\n", index).unwrap_or(text.len());
            continue;
        }
        if rest.starts_with(b"/*") {
            found.push(line_at(text, index));
            index = find(text, b"*/", index + 2).map_or(text.len(), |end| end + 2);
            continue;
        }
        let at_word_start = index == 0 || !is_ident(text[index - 1]);
        if at_word_start && let Some(end) = raw_string_end(text, index) {
            index = end;
            continue;
        }
        match text[index] {
            b'"' => {
                index += 1;
                while index < text.len() && text[index] != b'"' {
                    index += if text[index] == b'\\' { 2 } else { 1 };
                }
                index += 1;
            }
            b'\'' => index = char_literal_end(source, index).unwrap_or(index + 1),
            _ => index += 1,
        }
    }
    found
}

pub fn css(source: &str) -> Vec<usize> {
    let text = source.as_bytes();
    let mut found = Vec::new();
    let mut from = 0;
    while let Some(start) = find(text, b"/*", from) {
        found.push(line_at(text, start));
        from = start + 2;
    }
    found
}

pub fn svelte(source: &str) -> Vec<usize> {
    let text = source.as_bytes();
    let mut found = Vec::new();
    let mut from = 0;
    while let Some(start) = find(text, b"<!--", from) {
        found.push(line_at(text, start));
        from = start + 4;
    }
    let mut from = 0;
    while let Some(open) = find(text, b"<style", from) {
        let Some(body) = find(text, b">", open).map(|end| end + 1) else {
            break;
        };
        let close = find(text, b"</style>", body).unwrap_or(text.len());
        let mut cursor = body;
        while let Some(start) = find(&text[..close], b"/*", cursor) {
            found.push(line_at(text, start));
            cursor = start + 2;
        }
        from = close;
    }
    found.sort_unstable();
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_rust_comments_but_not_comment_lookalikes_in_strings() {
        let source = "fn a<'a>(x: &'a str) {\n    let t = \"a;//b\";\n    let r = r#\"x // \"y\" \"#;\n    let c = '/';\n    let d = '\"';\n    f(1, // c\n    );\n    let u = 1; /* c */\n    let e = '\\'';\n    let raw = cr#\"\\\"#; // hidden\n}\n";
        assert_eq!(rust(source), vec![6, 8, 10]);
    }

    #[test]
    fn urls_and_doc_comments() {
        assert_eq!(
            rust("let url = \"https://example.com\";\n"),
            Vec::<usize>::new()
        );
        assert_eq!(rust("/// doc\nfn a() {}\n"), vec![1]);
        assert_eq!(
            rust("#![cfg_attr(test, allow(dead_code))]\n"),
            Vec::<usize>::new()
        );
    }

    #[test]
    fn escaped_quote_char_does_not_hide_later_comments() {
        assert_eq!(rust("let a = ['\\'','\"']; let s = 1;\n// c2\n"), vec![2]);
        assert_eq!(rust("let e = '\\u{10FFFF}'; // c\n"), vec![1]);
        assert_eq!(rust("let c = '中'; // c\n"), vec![1]);
    }

    #[test]
    fn lifetimes_are_not_char_literals() {
        assert_eq!(
            rust("fn f<'a, 'b>(x: &'a str, y: &'static str) {}\n// c\n"),
            vec![2]
        );
    }

    #[test]
    fn svelte_markup_and_style_comments() {
        let source = "<div></div>\n<!-- x -->\n<style>\n a{} /* y */\n</style>\n<script>let u = \"/*\";</script>\n";
        assert_eq!(svelte(source), vec![2, 4]);
    }

    #[test]
    fn css_comments() {
        assert_eq!(css("a {}\n/* c */\n"), vec![2]);
    }
}
