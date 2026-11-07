//! Snippets in the LSP/TextMate syntax: `$1`, `${2:default}`, `${3|a,b|}`, `$0`.

use crate::buffer::ChangeRec;
use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Parsed {
    pub text: String,
    /// Tab stop number → ranges (chars, relative to the start of the text).
    pub stops: BTreeMap<u32, Vec<(usize, usize)>>,
}

pub fn parse(src: &str, vars: &dyn Fn(&str) -> Option<String>) -> Parsed {
    let chars: Vec<char> = src.chars().collect();
    let mut p = Parsed::default();
    let mut count = 0;
    parse_into(&chars, &mut 0, &mut p, &mut count, vars, false);
    p
}

fn parse_into(
    s: &[char],
    i: &mut usize,
    p: &mut Parsed,
    count: &mut usize,
    vars: &dyn Fn(&str) -> Option<String>,
    nested: bool,
) {
    while *i < s.len() {
        let c = s[*i];
        match c {
            '\\' if *i + 1 < s.len() && matches!(s[*i + 1], '$' | '}' | '\\') => {
                p.text.push(s[*i + 1]);
                *count += 1;
                *i += 2;
            }
            '}' if nested => return,
            '$' if *i + 1 < s.len() => {
                let next = s[*i + 1];
                if next.is_ascii_digit() {
                    *i += 1;
                    let n = read_num(s, i);
                    p.stops.entry(n).or_default().push((*count, *count));
                } else if next == '{' {
                    *i += 2;
                    placeholder(s, i, p, count, vars);
                } else if next.is_alphabetic() || next == '_' {
                    *i += 1;
                    let name = read_name(s, i);
                    if let Some(v) = vars(&name) {
                        *count += v.chars().count();
                        p.text.push_str(&v);
                    }
                } else {
                    p.text.push('$');
                    *count += 1;
                    *i += 1;
                }
            }
            _ => {
                p.text.push(c);
                *count += 1;
                *i += 1;
            }
        }
    }
}

fn placeholder(
    s: &[char],
    i: &mut usize,
    p: &mut Parsed,
    count: &mut usize,
    vars: &dyn Fn(&str) -> Option<String>,
) {
    if *i < s.len() && s[*i].is_ascii_digit() {
        let n = read_num(s, i);
        let start = *count;
        match s.get(*i) {
            Some(':') => {
                *i += 1;
                parse_into(s, i, p, count, vars, true);
            }
            Some('|') => {
                // A choice: the first option is what gets typed.
                *i += 1;
                let mut first = true;
                while *i < s.len() && s[*i] != '|' {
                    if s[*i] == ',' {
                        first = false;
                    } else if first {
                        p.text.push(s[*i]);
                        *count += 1;
                    }
                    *i += 1;
                }
                *i += 1;
            }
            _ => {}
        }
        if s.get(*i) == Some(&'}') {
            *i += 1;
        }
        p.stops.entry(n).or_default().push((start, *count));
    } else {
        let name = read_name(s, i);
        let value = vars(&name);
        if s.get(*i) == Some(&':') {
            *i += 1;
            if let Some(v) = value {
                *count += v.chars().count();
                p.text.push_str(&v);
                let mut sink = Parsed::default();
                let mut c2 = 0;
                parse_into(s, i, &mut sink, &mut c2, vars, true);
            } else {
                parse_into(s, i, p, count, vars, true);
            }
        } else if let Some(v) = value {
            *count += v.chars().count();
            p.text.push_str(&v);
        }
        if s.get(*i) == Some(&'}') {
            *i += 1;
        }
    }
}

fn read_num(s: &[char], i: &mut usize) -> u32 {
    let mut n = 0u32;
    while *i < s.len() && s[*i].is_ascii_digit() {
        n = n.saturating_mul(10).saturating_add(s[*i].to_digit(10).unwrap());
        *i += 1;
    }
    n
}

fn read_name(s: &[char], i: &mut usize) -> String {
    let mut out = String::new();
    while *i < s.len() && (s[*i].is_alphanumeric() || s[*i] == '_') {
        out.push(s[*i]);
        *i += 1;
    }
    out
}

/// A snippet being filled in: Tab walks through its stops.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    /// In order, `$0` last. Absolute char ranges in the document.
    stops: Vec<Vec<(usize, usize)>>,
    current: usize,
}

impl Session {
    /// `base` is where the snippet's text went in. Returns `None` if it has no stop to visit.
    pub fn new(parsed: &Parsed, base: usize) -> Option<Self> {
        let mut stops: Vec<Vec<(usize, usize)>> = parsed
            .stops
            .iter()
            .filter(|(n, _)| **n != 0)
            .map(|(_, r)| r.iter().map(|(a, b)| (a + base, b + base)).collect())
            .collect();
        let end = parsed
            .stops
            .get(&0)
            .map(|r| r.iter().map(|(a, b)| (a + base, b + base)).collect())
            .unwrap_or_else(|| {
                let e = base + parsed.text.chars().count();
                vec![(e, e)]
            });
        if stops.is_empty() {
            return None;
        }
        stops.push(end);
        Some(Self { stops, current: 0 })
    }

    pub fn current(&self) -> &[(usize, usize)] {
        &self.stops[self.current]
    }

    /// Moves on. `None` means the snippet is finished (the cursor is left at the final stop).
    pub fn next(&mut self) -> Option<&[(usize, usize)]> {
        if self.current + 1 < self.stops.len() {
            self.current += 1;
            Some(self.current())
        } else {
            None
        }
    }

    pub fn prev(&mut self) -> Option<&[(usize, usize)]> {
        if self.current > 0 {
            self.current -= 1;
            Some(self.current())
        } else {
            None
        }
    }

    pub fn is_last(&self) -> bool {
        self.current + 1 == self.stops.len()
    }

    /// Follows edits so later stops stay put. False when the edit left the snippet behind.
    pub fn on_change(&mut self, c: &ChangeRec) -> bool {
        let old_end = c.at + c.removed;
        let delta = c.inserted as isize - c.removed as isize;
        let cur = self.current;
        let mut inside_current = false;
        for (i, group) in self.stops.iter_mut().enumerate() {
            for r in group.iter_mut() {
                if i == cur && r.0 <= c.at && old_end <= r.1 {
                    inside_current = true;
                    r.1 = (r.1 as isize + delta) as usize;
                } else if r.0 >= old_end {
                    // Stops at the very edit point of the active one stay behind it.
                    r.0 = (r.0 as isize + delta) as usize;
                    r.1 = (r.1 as isize + delta) as usize;
                } else if r.1 > c.at {
                    r.1 = r.1.max(old_end);
                    r.1 = (r.1 as isize + delta) as usize;
                    r.0 = r.0.min(c.at);
                }
            }
        }
        inside_current || self.stops[cur].iter().any(|r| r.0 >= c.at + c.inserted)
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct Definition {
    pub prefix: String,
    pub description: String,
    pub body: String,
}

fn def(prefix: &str, description: &str, body: &str) -> Definition {
    Definition { prefix: prefix.into(), description: description.into(), body: body.into() }
}

/// Snippets that come with the editor. Users add their own in `snippets.json`.
pub fn builtin(language: &str) -> Vec<Definition> {
    match language {
        "Rust" => vec![
            def("fn", "function", "fn ${1:name}($2) ${3:-> ()} {\n    $0\n}"),
            def("pfn", "public function", "pub fn ${1:name}($2) ${3:-> ()} {\n    $0\n}"),
            def("struct", "struct", "struct ${1:Name} {\n    $0\n}"),
            def("enum", "enum", "enum ${1:Name} {\n    $0\n}"),
            def("impl", "impl block", "impl ${1:Type} {\n    $0\n}"),
            def("test", "test function", "#[test]\nfn ${1:name}() {\n    $0\n}"),
            def("match", "match", "match ${1:value} {\n    ${2:_} => $0,\n}"),
            def("for", "for loop", "for ${1:item} in ${2:iter} {\n    $0\n}"),
            def("derive", "derive", "#[derive(${1:Debug, Clone})]"),
            def("main", "main function", "fn main() {\n    $0\n}"),
        ],
        "Python" => vec![
            def("def", "function", "def ${1:name}($2):\n    ${0:pass}"),
            def("class", "class", "class ${1:Name}:\n    def __init__(self$2):\n        ${0:pass}"),
            def("for", "for loop", "for ${1:item} in ${2:items}:\n    ${0:pass}"),
            def("ifmain", "main guard", "if __name__ == \"__main__\":\n    ${0:main()}"),
            def("try", "try/except", "try:\n    $1\nexcept ${2:Exception} as e:\n    ${0:raise}"),
            def("with", "with block", "with ${1:open(path)} as ${2:f}:\n    ${0:pass}"),
        ],
        "C" | "C++" => vec![
            def("main", "main function", "int main(int argc, char *argv[]) {\n    $0\n    return 0;\n}"),
            def("for", "for loop", "for (int ${1:i} = 0; $1 < ${2:n}; $1++) {\n    $0\n}"),
            def("if", "if", "if (${1:cond}) {\n    $0\n}"),
            def("while", "while", "while (${1:cond}) {\n    $0\n}"),
            def("struct", "struct", "struct ${1:name} {\n    $0\n};"),
            def("inc", "include", "#include <${1:stdio.h}>"),
            def("guard", "include guard", "#ifndef ${1:NAME_H}\n#define $1\n\n$0\n\n#endif"),
        ],
        "JavaScript" | "TypeScript" | "JSX" | "TypeScriptReact" | "JavaScript (Babel)" => vec![
            def("fn", "function", "function ${1:name}($2) {\n    $0\n}"),
            def("af", "arrow function", "const ${1:name} = ($2) => {\n    $0\n};"),
            def("cl", "console.log", "console.log($0);"),
            def("for", "for...of", "for (const ${1:item} of ${2:items}) {\n    $0\n}"),
            def("class", "class", "class ${1:Name} {\n    constructor($2) {\n        $0\n    }\n}"),
            def("imp", "import", "import { $2 } from \"${1:module}\";"),
        ],
        "Go" => vec![
            def("func", "function", "func ${1:name}($2) ${3:error} {\n\t$0\n}"),
            def("iferr", "if err", "if err != nil {\n\treturn ${1:err}\n}"),
            def("for", "range loop", "for ${1:i}, ${2:v} := range ${3:items} {\n\t$0\n}"),
            def("struct", "struct", "type ${1:Name} struct {\n\t$0\n}"),
        ],
        "HTML" => vec![
            def("html", "document", "<!doctype html>\n<html lang=\"${1:pt-BR}\">\n<head>\n    <meta charset=\"utf-8\">\n    <title>${2:Title}</title>\n</head>\n<body>\n    $0\n</body>\n</html>"),
            def("div", "div", "<div class=\"$1\">$0</div>"),
            def("a", "link", "<a href=\"$1\">$0</a>"),
        ],
        "Markdown" => vec![
            def("code", "code block", "```${1:rust}\n$0\n```"),
            def("link", "link", "[${1:text}](${2:url})"),
            def("table", "table", "| ${1:A} | ${2:B} |\n| --- | --- |\n| $3 | $0 |"),
        ],
        "Bourne Again Shell (bash)" => vec![
            def("sh", "shebang", "#!/usr/bin/env bash\nset -euo pipefail\n\n$0"),
            def("for", "for loop", "for ${1:item} in ${2:items}; do\n    $0\ndone"),
            def("if", "if", "if [[ ${1:cond} ]]; then\n    $0\nfi"),
        ],
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> Parsed {
        parse(s, &|n| (n == "TM_FILENAME").then(|| "a.rs".to_string()))
    }

    #[test]
    fn parses_stops_defaults_and_choices() {
        let r = p("fn ${1:name}($2) -> ${3|i32,u8|} {$0}");
        assert_eq!(r.text, "fn name() -> i32 {}");
        assert_eq!(r.stops[&1], vec![(3, 7)]);
        assert_eq!(r.stops[&2], vec![(8, 8)]);
        assert_eq!(r.stops[&3], vec![(13, 16)]);
        assert_eq!(r.stops[&0], vec![(18, 18)]);
    }

    #[test]
    fn variables_escapes_and_mirrors() {
        let r = p("$TM_FILENAME \\$1 ${UNKNOWN:x} for $1 $1");
        assert_eq!(r.text, "a.rs $1 x for  ");
        assert_eq!(r.stops[&1].len(), 2);
    }

    #[test]
    fn nested_placeholder_keeps_outer_range() {
        let r = p("${1:a${2:b}c}");
        assert_eq!(r.text, "abc");
        assert_eq!(r.stops[&1], vec![(0, 3)]);
        assert_eq!(r.stops[&2], vec![(1, 2)]);
    }

    #[test]
    fn session_walks_stops_and_follows_typing() {
        let parsed = p("f(${1:a}, ${2:b})$0");
        let mut s = Session::new(&parsed, 10).unwrap();
        assert_eq!(s.current(), &[(12, 13)]);
        // Typing "xyz" over the first stop.
        assert!(s.on_change(&ChangeRec { at: 12, removed: 1, inserted: 3, start_line: 0, removed_lines: 0, inserted_lines: 0, ..Default::default() }));
        assert_eq!(s.current(), &[(12, 15)]);
        assert_eq!(s.next().unwrap(), &[(17, 18)]);
        assert_eq!(s.next().unwrap(), &[(19, 19)]);
        assert!(s.is_last());
        assert!(s.next().is_none());
    }

    #[test]
    fn no_stops_means_no_session() {
        assert!(Session::new(&p("plain"), 0).is_none());
        assert!(Session::new(&p("plain$0"), 0).is_none());
    }

    #[test]
    fn builtins_parse_for_the_main_languages() {
        for l in ["Rust", "Python", "C", "TypeScript", "Go", "HTML", "Markdown"] {
            let defs = builtin(l);
            assert!(!defs.is_empty(), "{l}");
            for d in defs {
                let parsed = p(&d.body);
                assert!(!parsed.text.contains("${"), "{}", d.body);
            }
        }
    }
}
