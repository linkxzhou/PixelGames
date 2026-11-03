//! A small XML reader for the SVG subset: elements, attributes, comments,
//! CDATA, processing instructions and a DOCTYPE without an internal subset.
//! Text content is ignored (the subset has no `text`). Entities: the five
//! predefined ones plus numeric references.

use pxl_core::{PxlError, PxlResult};

#[derive(Debug, Clone, Default)]
pub struct Node {
    pub name: String,
    pub attrs: Vec<(String, String)>,
    pub children: Vec<Node>,
}

impl Node {
    pub fn attr(&self, key: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }

    /// Visit every node, depth first.
    pub fn walk<'a>(&'a self, f: &mut dyn FnMut(&'a Node)) {
        f(self);
        for c in &self.children {
            c.walk(f);
        }
    }
}

/// Nesting deeper than this is refused rather than recursed into.
const MAX_DEPTH: usize = 64;
const MAX_NODES: usize = 50_000;

pub fn parse(text: &str) -> PxlResult<Node> {
    let mut p = Parser {
        s: text.as_bytes(),
        i: 0,
        nodes: 0,
    };
    p.skip_prolog()?;
    let root = p.element(0)?;
    Ok(root)
}

struct Parser<'a> {
    s: &'a [u8],
    i: usize,
    nodes: usize,
}

fn err(msg: impl Into<String>) -> PxlError {
    PxlError::schema(format!("svg.xml: {}", msg.into()))
        .with_hint("the SVG must be well-formed XML with one root <svg> element")
}

impl<'a> Parser<'a> {
    fn starts(&self, pat: &str) -> bool {
        self.s[self.i..].starts_with(pat.as_bytes())
    }

    fn skip_ws(&mut self) {
        while self.i < self.s.len() && self.s[self.i].is_ascii_whitespace() {
            self.i += 1;
        }
    }

    fn skip_until(&mut self, pat: &str) -> PxlResult<()> {
        let p = pat.as_bytes();
        while self.i + p.len() <= self.s.len() {
            if &self.s[self.i..self.i + p.len()] == p {
                self.i += p.len();
                return Ok(());
            }
            self.i += 1;
        }
        Err(err(format!("missing '{pat}'")))
    }

    /// Comments, PIs and DOCTYPE before and between elements.
    fn skip_misc(&mut self) -> PxlResult<bool> {
        self.skip_ws();
        if self.starts("<!--") {
            self.skip_until("-->")?;
            Ok(true)
        } else if self.starts("<?") {
            self.skip_until("?>")?;
            Ok(true)
        } else if self.starts("<!DOCTYPE") || self.starts("<!doctype") {
            if self.s[self.i..].contains(&b'[') {
                let close = self.s[self.i..].iter().position(|b| *b == b'>');
                let open = self.s[self.i..].iter().position(|b| *b == b'[');
                if matches!((open, close), (Some(o), Some(c)) if o < c) {
                    return Err(err("a DOCTYPE with an internal subset is not supported"));
                }
            }
            self.skip_until(">")?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn skip_prolog(&mut self) -> PxlResult<()> {
        if self.starts("\u{feff}") {
            self.i += 3;
        }
        while self.skip_misc()? {}
        Ok(())
    }

    fn name(&mut self) -> PxlResult<String> {
        let start = self.i;
        while self.i < self.s.len() {
            let b = self.s[self.i];
            if b.is_ascii_alphanumeric() || matches!(b, b':' | b'-' | b'_' | b'.') || b >= 0x80 {
                self.i += 1;
            } else {
                break;
            }
        }
        if start == self.i {
            return Err(err(format!("expected a name at byte {start}")));
        }
        Ok(String::from_utf8_lossy(&self.s[start..self.i]).into_owned())
    }

    fn element(&mut self, depth: usize) -> PxlResult<Node> {
        if depth > MAX_DEPTH {
            return Err(err(format!("nesting deeper than {MAX_DEPTH}")));
        }
        self.nodes += 1;
        if self.nodes > MAX_NODES {
            return Err(err(format!("more than {MAX_NODES} elements")));
        }
        if self.i >= self.s.len() || self.s[self.i] != b'<' {
            return Err(err(format!("expected '<' at byte {}", self.i)));
        }
        self.i += 1;
        let name = self.name()?;
        let mut node = Node {
            name,
            ..Default::default()
        };
        loop {
            self.skip_ws();
            if self.starts("/>") {
                self.i += 2;
                return Ok(node);
            }
            if self.starts(">") {
                self.i += 1;
                break;
            }
            let key = self.name()?;
            self.skip_ws();
            if !self.starts("=") {
                return Err(err(format!("attribute '{key}' has no value")));
            }
            self.i += 1;
            self.skip_ws();
            let q = *self
                .s
                .get(self.i)
                .ok_or_else(|| err("unexpected end in an attribute"))?;
            if q != b'"' && q != b'\'' {
                return Err(err(format!("attribute '{key}' value is not quoted")));
            }
            self.i += 1;
            let start = self.i;
            while self.i < self.s.len() && self.s[self.i] != q {
                self.i += 1;
            }
            if self.i >= self.s.len() {
                return Err(err(format!("attribute '{key}' is not closed")));
            }
            let raw = String::from_utf8_lossy(&self.s[start..self.i]).into_owned();
            self.i += 1;
            node.attrs.push((key, decode_entities(&raw)));
        }
        loop {
            // Text, comments, CDATA and children until the closing tag.
            while self.i < self.s.len() && self.s[self.i] != b'<' {
                self.i += 1;
            }
            if self.i >= self.s.len() {
                return Err(err(format!("<{}> is not closed", node.name)));
            }
            if self.starts("</") {
                self.i += 2;
                let close = self.name()?;
                if close != node.name {
                    return Err(err(format!("</{close}> closes <{}>", node.name)));
                }
                self.skip_ws();
                if !self.starts(">") {
                    return Err(err(format!("</{close}> is not closed")));
                }
                self.i += 1;
                return Ok(node);
            }
            if self.starts("<![CDATA[") {
                self.skip_until("]]>")?;
                continue;
            }
            if self.skip_misc()? {
                continue;
            }
            let child = self.element(depth + 1)?;
            node.children.push(child);
        }
    }
}

pub fn decode_entities(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(pos) = rest.find('&') {
        out.push_str(&rest[..pos]);
        let tail = &rest[pos..];
        let Some(end) = tail.find(';') else {
            out.push_str(tail);
            return out;
        };
        let ent = &tail[1..end];
        let ch = match ent {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            _ if ent.starts_with("#x") || ent.starts_with("#X") => {
                u32::from_str_radix(&ent[2..], 16)
                    .ok()
                    .and_then(char::from_u32)
            }
            _ if ent.starts_with('#') => ent[1..].parse::<u32>().ok().and_then(char::from_u32),
            _ => None,
        };
        match ch {
            Some(c) => out.push(c),
            None => out.push_str(&tail[..=end]),
        }
        rest = &tail[end + 1..];
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_elements_attributes_and_skips_noise() {
        let doc = r##"<?xml version="1.0"?>
<!-- c --><!DOCTYPE svg>
<svg viewBox='0 0 8 8' xmlns="http://www.w3.org/2000/svg">
  <g id="a"><rect x="1" y="1" width="2" height="2" fill="#f00"/>text<![CDATA[ <x> ]]></g>
  <circle r="1&amp;2"/>
</svg>"##;
        let n = parse(doc).unwrap();
        assert_eq!(n.name, "svg");
        assert_eq!(n.attr("viewBox"), Some("0 0 8 8"));
        assert_eq!(n.children.len(), 2);
        assert_eq!(n.children[0].children[0].name, "rect");
        assert_eq!(n.children[1].attr("r"), Some("1&2"));
    }

    #[test]
    fn malformed_input_is_a_schema_error() {
        for bad in [
            "<svg>",
            "<svg><g></svg>",
            "<svg a=1/>",
            "<svg a='1/>",
            "plain",
        ] {
            let e = parse(bad).unwrap_err();
            assert!(e.message.starts_with("svg.xml"), "{bad}: {}", e.message);
        }
    }

    #[test]
    fn entities() {
        assert_eq!(decode_entities("a&lt;b&#65;&#x42;&bogus;"), "a<bAB&bogus;");
    }
}
