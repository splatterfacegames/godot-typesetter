//! Rich-text tag tokenizer. Recognises the tag syntax documented for TextMesh
//! Pro (`<b>`, `<color=#RRGGBB>`, `<size=150%>`, `<link="id">`, `<sprite name="x">`,
//! closing tags `</b>`, and `<noparse>` regions) and leaves anything else as
//! literal text. Whether a syntactically valid tag is *accepted* (its value
//! parses, the named font / style / sprite exists) is decided by the layout
//! engine, which falls back to the tag's literal text when it is not.
//!
//! Syntax rules (black-box measurements of TMP):
//! * no white space directly after `<` or `</` (`< b>` is text);
//! * `=` must follow the tag name directly (`<size = 40>` is text);
//! * white space before `>` is allowed (`<b >`);
//! * values may be double-quoted; single quotes are ordinary characters;
//! * names are case-insensitive;
//! * only some tags have closing forms (`</alpha>`, `</space>`, `</br>`,
//!   `</margin-left>` … are text).

#[derive(Clone, Debug, PartialEq)]
pub struct Tag {
    /// Lower-cased tag name without '/' (the bare colour shorthand `<#f00>`
    /// becomes "color").
    pub name: String,
    pub closing: bool,
    /// Value after '=' on the tag name (quotes removed), if any.
    pub value: Option<String>,
    /// Other attributes, lower-cased keys: `<sprite name="x" tint=1>`.
    pub attrs: Vec<(String, String)>,
    /// Source span (start, end exclusive) in chars.
    pub span: (usize, usize),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Token {
    /// A character, its index in the source string (chars) and the number of
    /// source chars it stands for, as TMP reports them: a `\uXXXX` escape is 6
    /// long; `\n` reports the backslash's index and length 1; an escaped
    /// backslash reports the second backslash.
    Char(char, usize, usize),
    Tag(Tag),
}

/// Opening tags TMP recognises.
const KNOWN: &[&str] = &[
    "b", "i", "u", "s", "color", "alpha", "size", "font", "font-weight", "material", "sprite", "link", "a", "mark", "sup",
    "sub", "cspace", "mspace", "voffset", "indent", "line-height", "line-indent", "margin", "margin-left", "margin-right",
    "align", "allcaps", "smallcaps", "lowercase", "uppercase", "nobr", "noparse", "page", "pos", "rotate", "space", "width",
    "gradient", "style", "br", "zwsp", "nbsp", "shy", "zwj", "action",
];

/// Tags that have a closing form.
const CLOSABLE: &[&str] = &[
    "pos", "width", "rotate", "gradient", "font-weight", "a", "mark", "cspace", "mspace", "voffset", "indent", "line-height",
    "line-indent", "align", "allcaps", "uppercase", "lowercase", "smallcaps", "nobr", "sup", "sub", "material", "link",
    "style", "font", "size", "color", "u", "s", "i", "b", "noparse", "margin", "action",
];

fn parse_tag(body: &str, start: usize, end: usize) -> Option<Tag> {
    let b = body.trim_end();
    if b.is_empty() || b.starts_with(char::is_whitespace) {
        return None;
    }
    if let Some(rest) = b.strip_prefix('/') {
        if rest.is_empty() || rest.starts_with(char::is_whitespace) {
            return None;
        }
        let name = rest.split(|c: char| c == '=' || c.is_whitespace()).next().unwrap_or("").to_ascii_lowercase();
        if !CLOSABLE.contains(&name.as_str()) {
            return None;
        }
        return Some(Tag { name, closing: true, value: None, attrs: vec![], span: (start, end) });
    }
    // Colour shorthand: <#RRGGBB> / <#RGB> / <#RGBA> / <#RRGGBBAA>. Only the
    // length is checked; digits are decoded leniently (see `parse_color`).
    if let Some(hex) = b.strip_prefix('#') {
        if matches!(hex.chars().count(), 3 | 4 | 6 | 8) && !hex.contains(char::is_whitespace) {
            return Some(Tag { name: "color".into(), closing: false, value: Some(format!("#{hex}")), attrs: vec![], span: (start, end) });
        }
        return None;
    }
    // Split into whitespace-separated parts, honouring double quotes.
    let mut parts: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    for c in b.chars() {
        if c == '"' {
            quoted = !quoted;
            cur.push(c);
        } else if c.is_whitespace() && !quoted {
            if !cur.is_empty() {
                parts.push(std::mem::take(&mut cur));
            }
        } else {
            cur.push(c);
        }
    }
    if !cur.is_empty() {
        parts.push(cur);
    }
    let unquote = |s: &str| -> String {
        if s.len() >= 2 && s.starts_with('"') && s.ends_with('"') {
            s[1..s.len() - 1].to_string()
        } else {
            s.to_string()
        }
    };
    let first = parts.first()?.clone();
    let (name, value) = match first.split_once('=') {
        Some((n, v)) => (n.to_ascii_lowercase(), Some(unquote(v))),
        None => (first.to_ascii_lowercase(), None),
    };
    if !KNOWN.contains(&name.as_str()) {
        return None;
    }
    let mut attrs = Vec::new();
    for p in &parts[1..] {
        if let Some((k, v)) = p.split_once('=') {
            attrs.push((k.to_ascii_lowercase(), unquote(v)));
        } else {
            attrs.push((p.to_ascii_lowercase(), String::new()));
        }
    }
    Some(Tag { name, closing: false, value, attrs, span: (start, end) })
}

/// Tokenizes `text`. With `rich_text` false every character is literal.
pub fn tokenize(text: &str, rich_text: bool) -> Vec<Token> {
    tokenize_with(text, rich_text, true)
}

/// Decodes a backslash escape at `chars[i]` (`\n`, `\t`, `\r`, `\\`,
/// `\uXXXX`, `\UXXXXXXXX`); returns the character and the escape's length.
fn escape(chars: &[char], i: usize) -> Option<(char, usize)> {
    let n = *chars.get(i + 1)?;
    let hex = |len: usize| -> Option<char> {
        let s: String = chars.get(i + 2..i + 2 + len)?.iter().collect();
        if s.chars().all(|c| c.is_ascii_hexdigit()) {
            char::from_u32(u32::from_str_radix(&s, 16).ok()?)
        } else {
            None
        }
    };
    match n {
        'n' => Some(('\n', 2)),
        't' => Some(('\t', 2)),
        'r' => Some(('\r', 2)),
        '\\' => Some(('\\', 2)),
        'u' => hex(4).map(|c| (c, 6)),
        'U' => hex(8).map(|c| (c, 10)),
        _ => None,
    }
}

/// Tokenizes `text`; `parse_escapes` enables backslash escapes (TMP's
/// parseCtrlCharacters), which apply with or without rich text.
pub fn tokenize_with(text: &str, rich_text: bool, parse_escapes: bool) -> Vec<Token> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::with_capacity(chars.len());
    let mut i = 0usize;
    let mut noparse = false;
    while i < chars.len() {
        let c = chars[i];
        if rich_text && c == '<' {
            // Find the closing '>' (tags never span '<'); quoted values may hold '>'.
            let mut j = i + 1;
            let mut quoted = false;
            while j < chars.len() && (quoted || (chars[j] != '>' && chars[j] != '<')) && j - i < 512 {
                if chars[j] == '"' {
                    quoted = !quoted;
                }
                j += 1;
            }
            if j < chars.len() && chars[j] == '>' {
                let body: String = chars[i + 1..j].iter().collect();
                if let Some(tag) = parse_tag(&body, i, j + 1) {
                    if noparse {
                        if tag.closing && tag.name == "noparse" {
                            noparse = false;
                            i = j + 1;
                            continue;
                        }
                    } else if tag.name == "noparse" && !tag.closing {
                        noparse = true;
                        i = j + 1;
                        continue;
                    } else {
                        out.push(Token::Tag(tag));
                        i = j + 1;
                        continue;
                    }
                }
            }
        }
        if c == '\\' && parse_escapes {
            if let Some((m, len)) = escape(&chars, i) {
                let (at, l) = match len {
                    6 | 10 => (i, len),
                    _ if m == '\\' => (i + 1, 1),
                    _ => (i, 1),
                };
                out.push(Token::Char(m, at, l));
                i += len;
                continue;
            }
        }
        out.push(Token::Char(c, i, 1));
        i += 1;
    }
    out
}

fn hex_digit(c: char) -> u32 {
    // TMP decodes any non-hex digit as 15 (`<#zzz>` is white).
    c.to_digit(16).unwrap_or(15)
}

/// Parses a colour value: `#RGB`, `#RGBA`, `#RRGGBB`, `#RRGGBBAA` (digits
/// decoded leniently), or one of the documented names (case-insensitive).
pub fn parse_color(v: &str) -> Option<[f32; 4]> {
    let v = v.trim().trim_matches('"');
    if let Some(hex) = v.strip_prefix('#') {
        let cs: Vec<char> = hex.chars().collect();
        let d1 = |c: char| (hex_digit(c) * 17) as f32 / 255.0;
        let d2 = |a: char, b: char| (hex_digit(a) * 16 + hex_digit(b)) as f32 / 255.0;
        return match cs.len() {
            3 => Some([d1(cs[0]), d1(cs[1]), d1(cs[2]), 1.0]),
            4 => Some([d1(cs[0]), d1(cs[1]), d1(cs[2]), d1(cs[3])]),
            6 => Some([d2(cs[0], cs[1]), d2(cs[2], cs[3]), d2(cs[4], cs[5]), 1.0]),
            8 => Some([d2(cs[0], cs[1]), d2(cs[2], cs[3]), d2(cs[4], cs[5]), d2(cs[6], cs[7])]),
            _ => None,
        };
    }
    Some(match v.to_ascii_lowercase().as_str() {
        "black" => [0.0, 0.0, 0.0, 1.0],
        "blue" => [0.0, 0.0, 1.0, 1.0],
        "green" => [0.0, 1.0, 0.0, 1.0],
        "orange" => [1.0, 128.0 / 255.0, 0.0, 1.0],
        "purple" => [160.0 / 255.0, 32.0 / 255.0, 240.0 / 255.0, 1.0],
        "red" => [1.0, 0.0, 0.0, 1.0],
        "white" => [1.0, 1.0, 1.0, 1.0],
        "yellow" => [1.0, 235.0 / 255.0, 4.0 / 255.0, 1.0],
        _ => return None,
    })
}

/// Parses `#AA` for `<alpha>` (lenient digits).
pub fn parse_alpha(v: &str) -> Option<f32> {
    let h: Vec<char> = v.strip_prefix('#')?.chars().collect();
    if h.len() != 2 {
        return None;
    }
    Some((hex_digit(h[0]) * 16 + hex_digit(h[1])) as f32 / 255.0)
}

/// A length value with its unit as written in a tag.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Length {
    /// Plain number or `px`: pixels (scaled to the current point size).
    Pixels(f32),
    /// `em` units: multiples of the current font size.
    Em(f32),
    /// Percentage (meaning depends on the tag: of font size, of line width…).
    Percent(f32),
}

/// Parses a tag's numeric value the way TMP does: a leading number (sign,
/// digits, one decimal point) followed by an optional unit (`px`, `em`, `%`).
/// Text that is not a number reads as 0 px (`<size=abc>` is size 0); an empty
/// value is rejected.
pub fn parse_length(v: &str) -> Option<Length> {
    let v = v.trim();
    if v.is_empty() {
        return None;
    }
    let bytes: Vec<char> = v.chars().collect();
    let mut k = 0;
    if k < bytes.len() && (bytes[k] == '-' || bytes[k] == '+') {
        k += 1;
    }
    while k < bytes.len() && (bytes[k].is_ascii_digit() || bytes[k] == '.') {
        k += 1;
    }
    let num: String = bytes[..k].iter().collect();
    let n: f32 = num.parse().unwrap_or(0.0);
    let unit: String = bytes[k..].iter().collect::<String>().trim().to_ascii_lowercase();
    Some(match unit.as_str() {
        "%" => Length::Percent(n),
        "em" => Length::Em(n),
        _ => Length::Pixels(n),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tags(t: &str) -> Vec<String> {
        tokenize(t, true)
            .into_iter()
            .filter_map(|x| match x {
                Token::Tag(t) => Some(format!("{}{}{}", if t.closing { "/" } else { "" }, t.name, t.value.map(|v| format!("={v}")).unwrap_or_default())),
                _ => None,
            })
            .collect()
    }

    fn text(t: &str) -> String {
        tokenize(t, true).into_iter().filter_map(|x| if let Token::Char(c, ..) = x { Some(c) } else { None }).collect()
    }

    #[test]
    fn basic_tags() {
        assert_eq!(tags("<b>x</b><color=#ff0000>y</color><#00f>z"), vec!["b", "/b", "color=#ff0000", "/color", "color=#00f"]);
        assert_eq!(text("<b>x</b> <size=50%>y"), "x y");
        assert_eq!(tags("<link=\"my id\">a</link>"), vec!["link=my id", "/link"]);
        assert_eq!(tags("<B>x</B>"), vec!["b", "/b"]);
    }

    #[test]
    fn unknown_and_broken_tags_are_literal() {
        assert_eq!(text("a <foo> b < c"), "a <foo> b < c");
        assert_eq!(text("1 < 2 > 0"), "1 < 2 > 0");
        assert_eq!(text("< b>x</ b>"), "< b>x</ b>");
        assert_eq!(text("<b >x</b >"), "x");
        assert_eq!(text("<size = 40>"), "");
        assert_eq!(tags("<size = 40>"), vec!["size"]);
    }

    #[test]
    fn closing_forms() {
        assert_eq!(text("a</alpha>b</space>c</br>d</margin-left>e</foo>"), "a</alpha>b</space>c</br>d</margin-left>e</foo>");
        assert_eq!(text("a</pos>b</width>c</margin>d</action>"), "abcd");
    }

    #[test]
    fn noparse_region() {
        assert_eq!(text("<noparse><b>bold</b></noparse>!"), "<b>bold</b>!");
    }

    #[test]
    fn escapes() {
        assert_eq!(text("a\\nb\\tc\\\\d\\x\\u00E9"), "a\nb\tc\\d\\x\u{e9}");
        assert_eq!(text("\\U0001F600"), "\u{1F600}");
        let plain: String = tokenize_with("a\\nb", true, false).into_iter().filter_map(|x| if let Token::Char(c, ..) = x { Some(c) } else { None }).collect();
        assert_eq!(plain, "a\\nb");
        let norich: String = tokenize_with("<b>\\n", false, true).into_iter().filter_map(|x| if let Token::Char(c, ..) = x { Some(c) } else { None }).collect();
        assert_eq!(norich, "<b>\n");
    }

    #[test]
    fn attributes() {
        let t = tokenize("<sprite name=\"smile\" tint=1>", true);
        let Token::Tag(tag) = &t[0] else { panic!() };
        assert_eq!(tag.name, "sprite");
        assert_eq!(tag.attrs, vec![("name".to_string(), "smile".to_string()), ("tint".to_string(), "1".to_string())]);
    }

    #[test]
    fn colours_and_lengths() {
        assert_eq!(parse_color("#FF000080"), Some([1.0, 0.0, 0.0, 128.0 / 255.0]));
        assert_eq!(parse_color("red"), Some([1.0, 0.0, 0.0, 1.0]));
        assert_eq!(parse_color("RED"), Some([1.0, 0.0, 0.0, 1.0]));
        assert_eq!(parse_color("#zzz"), Some([1.0, 1.0, 1.0, 1.0]));
        assert_eq!(parse_color("#12345"), None);
        assert_eq!(parse_color("redd"), None);
        assert_eq!(parse_alpha("#80"), Some(128.0 / 255.0));
        assert_eq!(parse_alpha("80"), None);
        assert_eq!(parse_length("150%"), Some(Length::Percent(150.0)));
        assert_eq!(parse_length("1.5em"), Some(Length::Em(1.5)));
        assert_eq!(parse_length("12"), Some(Length::Pixels(12.0)));
        assert_eq!(parse_length("10px"), Some(Length::Pixels(10.0)));
        assert_eq!(parse_length("abc"), Some(Length::Pixels(0.0)));
        assert_eq!(parse_length(""), None);
    }
}
