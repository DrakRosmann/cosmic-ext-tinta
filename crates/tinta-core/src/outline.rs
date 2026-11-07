//! Symbols in a file by pattern, for the outline and `@` in the palette. LSP's document symbols,
//! when a server is running, replace this; the patterns keep it useful everywhere else.

use crate::buffer::Document;
use regex::Regex;
use std::sync::OnceLock;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Symbol {
    pub name: String,
    pub kind: &'static str,
    pub line: usize,
    pub depth: usize,
    /// Last line, when known from the syntax tree.
    pub end: Option<usize>,
}

type Pat = (&'static str, Regex);

fn r(kind: &'static str, p: &str) -> Pat {
    (kind, Regex::new(p).expect("valid pattern"))
}

fn rust() -> &'static [Pat] {
    static P: OnceLock<Vec<Pat>> = OnceLock::new();
    P.get_or_init(|| {
        let vis = r"^\s*(?:pub(?:\([^)]*\))?\s+)?";
        vec![
            r("fn", &format!(r"{vis}(?:(?:const|async|unsafe|extern(?:\s+\S+)?)\s+)*fn\s+([A-Za-z_]\w*)")),
            r("struct", &format!(r"{vis}struct\s+([A-Za-z_]\w*)")),
            r("enum", &format!(r"{vis}enum\s+([A-Za-z_]\w*)")),
            r("trait", &format!(r"{vis}(?:unsafe\s+)?trait\s+([A-Za-z_]\w*)")),
            r("impl", r"^\s*(?:unsafe\s+)?impl(?:<[^>]*>)?\s+([^{]+?)\s*(?:\{|$|where)"),
            r("mod", &format!(r"{vis}mod\s+([A-Za-z_]\w*)")),
            r("const", &format!(r"{vis}(?:const|static)\s+(?:mut\s+)?([A-Za-z_]\w*)\s*:")),
            r("type", &format!(r"{vis}type\s+([A-Za-z_]\w*)")),
            r("macro", r"^\s*macro_rules!\s+([A-Za-z_]\w*)"),
        ]
    })
}

fn python() -> &'static [Pat] {
    static P: OnceLock<Vec<Pat>> = OnceLock::new();
    P.get_or_init(|| {
        vec![
            r("class", r"^\s*class\s+([A-Za-z_]\w*)"),
            r("fn", r"^\s*(?:async\s+)?def\s+([A-Za-z_]\w*)"),
        ]
    })
}

fn clike() -> &'static [Pat] {
    static P: OnceLock<Vec<Pat>> = OnceLock::new();
    P.get_or_init(|| {
        vec![
            r("class", r"^\s*(?:template\s*<[^>]*>\s*)?(?:class|struct|union|enum(?:\s+class)?|namespace)\s+([A-Za-z_]\w*)"),
            r("macro", r"^\s*#\s*define\s+([A-Za-z_]\w*)"),
            r("fn", r"^[A-Za-z_][\w\s\*&:<>,~]*?[\s\*&]([A-Za-z_~][\w:~]*)\s*\([^;]*$"),
        ]
    })
}

fn js() -> &'static [Pat] {
    static P: OnceLock<Vec<Pat>> = OnceLock::new();
    P.get_or_init(|| {
        vec![
            r("class", r"^\s*(?:export\s+)?(?:default\s+)?(?:abstract\s+)?class\s+([A-Za-z_$][\w$]*)"),
            r("interface", r"^\s*(?:export\s+)?(?:interface|type|enum)\s+([A-Za-z_$][\w$]*)"),
            r("fn", r"^\s*(?:export\s+)?(?:default\s+)?(?:async\s+)?function\s*\*?\s*([A-Za-z_$][\w$]*)"),
            r("fn", r"^\s*(?:export\s+)?(?:const|let|var)\s+([A-Za-z_$][\w$]*)\s*(?::[^=]+)?=\s*(?:async\s*)?(?:\([^)]*\)|[A-Za-z_$][\w$]*)\s*(?::[^=]+)?=>"),
            r("fn", r"^\s+(?:public\s+|private\s+|protected\s+|static\s+|async\s+)*([A-Za-z_$][\w$]*)\s*\([^)]*\)\s*(?::[^{]+)?\{\s*$"),
        ]
    })
}

fn go() -> &'static [Pat] {
    static P: OnceLock<Vec<Pat>> = OnceLock::new();
    P.get_or_init(|| {
        vec![
            r("fn", r"^func\s+(?:\([^)]*\)\s*)?([A-Za-z_]\w*)"),
            r("type", r"^type\s+([A-Za-z_]\w*)"),
        ]
    })
}

fn java() -> &'static [Pat] {
    static P: OnceLock<Vec<Pat>> = OnceLock::new();
    P.get_or_init(|| {
        vec![
            r("class", r"^\s*(?:(?:public|private|protected|static|final|abstract|sealed|internal|data|open)\s+)*(?:class|interface|enum|record|object)\s+([A-Za-z_]\w*)"),
            r("fn", r"^\s*(?:(?:public|private|protected|static|final|abstract|synchronized|override|suspend|internal)\s+)*(?:fun\s+|[\w<>\[\],\s]+\s+)([A-Za-z_]\w*)\s*\([^;]*$"),
        ]
    })
}

fn shell() -> &'static [Pat] {
    static P: OnceLock<Vec<Pat>> = OnceLock::new();
    P.get_or_init(|| {
        vec![r("fn", r"^\s*(?:function\s+)?([A-Za-z_][\w-]*)\s*\(\)"), r("fn", r"^\s*function\s+([A-Za-z_][\w-]*)")]
    })
}

fn generic() -> &'static [Pat] {
    static P: OnceLock<Vec<Pat>> = OnceLock::new();
    P.get_or_init(|| {
        vec![
            r("fn", r"^\s*(?:local\s+|pub\s+|export\s+)?(?:function|func|fn|def|sub|proc)\s+([A-Za-z_][\w.:]*)"),
            r("class", r"^\s*(?:class|struct|module|interface|enum|trait)\s+([A-Za-z_][\w.:]*)"),
        ]
    })
}

fn patterns(language: &str) -> &'static [Pat] {
    match language {
        "Rust" => rust(),
        "Python" => python(),
        "C" | "C++" | "Objective-C" | "Objective-C++" => clike(),
        "JavaScript" | "TypeScript" | "JSX" | "TypeScriptReact" | "JavaScript (Babel)" | "Vue Component" | "Svelte" => js(),
        "Go" => go(),
        "Java" | "Kotlin" | "C#" | "Scala" | "Dart" | "Swift" => java(),
        "Bourne Again Shell (bash)" | "fish" => shell(),
        _ => generic(),
    }
}

const NOT_FUNCTIONS: &[&str] = &["if", "for", "while", "switch", "catch", "return", "else", "do", "sizeof", "new", "throw"];

pub fn symbols(doc: &Document) -> Vec<Symbol> {
    if let Some(st) = &doc.structure
        && st.is_current(doc.revision)
    {
        return st.symbols().to_vec();
    }
    let lang = doc.language.name.as_str();
    let total = doc.len_lines().min(200_000);
    let mut out = Vec::new();
    if lang == "Markdown" || lang == "MultiMarkdown" {
        let mut fenced = false;
        for l in 0..total {
            let text = doc.line_text(l);
            if text.trim_start().starts_with("```") {
                fenced = !fenced;
            }
            if fenced {
                continue;
            }
            let hashes = text.chars().take_while(|c| *c == '#').count();
            if (1..=6).contains(&hashes) && text.chars().nth(hashes) == Some(' ') {
                out.push(Symbol {
                    name: text[hashes..].trim().to_string(),
                    kind: "heading",
                    line: l,
                    depth: hashes - 1,
                    end: None,
                });
            }
        }
        return out;
    }
    let pats = patterns(lang);
    let tab = doc.indent.width.max(1);
    for l in 0..total {
        let text = doc.line_text(l);
        if text.len() > 500 {
            continue;
        }
        for (kind, re) in pats {
            if let Some(c) = re.captures(&text) {
                let name = c.get(1).map(|m| m.as_str().trim()).unwrap_or("");
                if name.is_empty() || NOT_FUNCTIONS.contains(&name) {
                    continue;
                }
                let indent = crate::display::line_indent_vcols(doc, l, tab);
                out.push(Symbol { name: name.to_string(), kind, line: l, depth: indent / tab, end: None });
                break;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::language;

    fn syms(lang: &str, src: &str) -> Vec<(String, &'static str)> {
        let mut d = Document::new(src);
        d.language = language::by_name(lang);
        symbols(&d).into_iter().map(|s| (s.name, s.kind)).collect()
    }

    #[test]
    fn rust_items() {
        let got = syms("Rust", "pub struct A;\nimpl A {\n    pub async fn run(&self) {}\n}\nconst X: u8 = 1;\nlet y = 1;\n");
        assert_eq!(got, vec![("A".into(), "struct"), ("A".into(), "impl"), ("run".into(), "fn"), ("X".into(), "const")]);
    }

    #[test]
    fn python_and_markdown() {
        let got = syms("Python", "class A:\n    def b(self):\n        pass\n");
        assert_eq!(got, vec![("A".into(), "class"), ("b".into(), "fn")]);
        let got = syms("Markdown", "# T\n```\n# not\n```\n## S\n");
        assert_eq!(got, vec![("T".into(), "heading"), ("S".into(), "heading")]);
    }

    #[test]
    fn c_functions_not_control_flow() {
        let got = syms("C", "#define M 1\nstatic int foo(int a) {\n    if (a) {\n    }\n}\nstruct s {\n};\n");
        assert!(got.contains(&("foo".into(), "fn")));
        assert!(got.contains(&("s".into(), "class")));
        assert!(!got.iter().any(|(n, _)| n == "if"));
    }

    #[test]
    fn js_ts() {
        let got = syms("TypeScript", "export function a() {}\nconst b = async (x) => 1;\nexport class C {\n  m(y) {\n  }\n}\ninterface I {}\n");
        let names: Vec<_> = got.iter().map(|g| g.0.as_str()).collect();
        assert_eq!(names, vec!["a", "b", "C", "m", "I"]);
    }
}
