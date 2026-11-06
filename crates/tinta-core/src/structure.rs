//! What a file means, from a tree-sitter parse: blocks to fold, symbols, what to expand a selection
//! to. Colours stay with the syntect grammars, which cover far more languages; a language without a
//! parser here just falls back on indentation and patterns.

use crate::outline::Symbol;
use ropey::Rope;
use std::collections::BTreeMap;
use tree_sitter::{InputEdit, Language, Node, Parser, Point, Tree};

/// Bigger than this, a first parse would be felt, and indentation works well enough.
pub const MAX_CHARS: usize = 3_000_000;

fn grammar(language: &str) -> Option<Language> {
    Some(match language {
        "Rust" => tree_sitter_rust::LANGUAGE.into(),
        "Python" => tree_sitter_python::LANGUAGE.into(),
        "C" => tree_sitter_c::LANGUAGE.into(),
        "C++" => tree_sitter_cpp::LANGUAGE.into(),
        "JavaScript" | "JavaScript (Babel)" | "JSX" => tree_sitter_javascript::LANGUAGE.into(),
        "TypeScript" => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        "TypeScriptReact" => tree_sitter_typescript::LANGUAGE_TSX.into(),
        "Go" => tree_sitter_go::LANGUAGE.into(),
        "Java" => tree_sitter_java::LANGUAGE.into(),
        "Bourne Again Shell (bash)" => tree_sitter_bash::LANGUAGE.into(),
        "JSON" => tree_sitter_json::LANGUAGE.into(),
        "HTML" => tree_sitter_html::LANGUAGE.into(),
        "CSS" => tree_sitter_css::LANGUAGE.into(),
        _ => return None,
    })
}

/// An edit's start, captured before the text changes.
pub struct EditStart {
    start_byte: usize,
    old_end_byte: usize,
    start_position: Point,
    old_end_position: Point,
}

fn point(rope: &Rope, byte: usize) -> Point {
    let line = rope.byte_to_line(byte);
    Point { row: line, column: byte - rope.line_to_byte(line) }
}

impl EditStart {
    pub fn new(rope: &Rope, at: usize, remove: usize) -> Self {
        let start_byte = rope.char_to_byte(at);
        let old_end_byte = rope.char_to_byte(at + remove);
        Self { start_byte, old_end_byte, start_position: point(rope, start_byte), old_end_position: point(rope, old_end_byte) }
    }

    pub fn finish(self, rope: &Rope, inserted_bytes: usize) -> InputEdit {
        let new_end_byte = self.start_byte + inserted_bytes;
        InputEdit {
            start_byte: self.start_byte,
            old_end_byte: self.old_end_byte,
            new_end_byte,
            start_position: self.start_position,
            old_end_position: self.old_end_position,
            new_end_position: point(rope, new_end_byte),
        }
    }
}

pub struct Structure {
    parser: Parser,
    tree: Option<Tree>,
    /// Header line → last line it can hide.
    folds: BTreeMap<usize, usize>,
    symbols: Vec<Symbol>,
    language: String,
    parsed_rev: Option<u64>,
}

impl std::fmt::Debug for Structure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Structure").field("language", &self.language).field("rev", &self.parsed_rev).finish()
    }
}

impl Structure {
    pub fn new(language: &str) -> Option<Self> {
        let g = grammar(language)?;
        let mut parser = Parser::new();
        parser.set_language(&g).ok()?;
        Some(Self { parser, tree: None, folds: BTreeMap::new(), symbols: Vec::new(), language: language.to_string(), parsed_rev: None })
    }

    pub fn edit(&mut self, e: InputEdit) {
        if let Some(t) = &mut self.tree {
            t.edit(&e);
        }
    }

    pub fn has_tree(&self) -> bool {
        self.tree.is_some()
    }

    pub fn is_current(&self, revision: u64) -> bool {
        self.parsed_rev == Some(revision)
    }

    /// Reparses what changed and rebuilds the lists.
    pub fn update(&mut self, rope: &Rope, revision: u64) {
        if self.parsed_rev == Some(revision) {
            return;
        }
        let mut read = |byte: usize, _: Point| -> &[u8] {
            if byte >= rope.len_bytes() {
                return &[];
            }
            let (chunk, start, _, _) = rope.chunk_at_byte(byte);
            &chunk.as_bytes()[byte - start..]
        };
        let tree = self.parser.parse_with_options(&mut read, self.tree.as_ref(), None);
        self.tree = tree;
        self.parsed_rev = Some(revision);
        self.rebuild(rope);
    }

    fn rebuild(&mut self, rope: &Rope) {
        self.folds.clear();
        self.symbols.clear();
        let Some(tree) = self.tree.take() else { return };
        {
            let root = tree.root_node();
            let mut cursor = root.walk();
            // Depth-first over named nodes, tracking how many symbols are open around each.
            let mut depth_stack: Vec<usize> = Vec::new();
            let mut symbol_depth = 0usize;
            'walk: loop {
                let node = cursor.node();
                let is_symbol = self.visit(node, root, rope, symbol_depth);
                if is_symbol {
                    symbol_depth += 1;
                }
                if cursor.goto_first_child() {
                    depth_stack.push(usize::from(is_symbol));
                    continue;
                }
                if is_symbol {
                    symbol_depth -= 1;
                }
                while !cursor.goto_next_sibling() {
                    if !cursor.goto_parent() {
                        break 'walk;
                    }
                    symbol_depth -= depth_stack.pop().unwrap_or(0);
                }
            }
        }
        self.tree = Some(tree);
    }

    fn visit(&mut self, node: Node<'_>, root: Node<'_>, rope: &Rope, depth: usize) -> bool {
        if !node.is_named() {
            return false;
        }
        if node.id() != root.id() {
            self.add_fold(node);
        }
        if let Some((name, kind)) = symbol_of(node, rope) {
            self.symbols.push(Symbol { name, kind, line: node.start_position().row, depth, end: Some(node.end_position().row) });
            return true;
        }
        false
    }

    fn add_fold(&mut self, node: Node<'_>) {
        let (start, end) = (node.start_position().row, node.end_position().row);
        if end <= start {
            return;
        }
        let kind = node.kind();
        // Python-style blocks have no closer and start on the line after their header.
        if matches!(kind, "block" | "suite") && node.child_count() > 0 && node.child(0).is_some_and(|c| c.is_named()) {
            if let Some(parent) = node.parent()
                && parent.start_position().row < start
            {
                self.insert_fold(start - 1, end);
            }
            return;
        }
        if kind.contains("comment") || kind.contains("string") {
            self.insert_fold(start, end);
            return;
        }
        // Otherwise, anything that ends with a closing token alone at the start of its line.
        let Some(last) = node.child(node.child_count().saturating_sub(1)) else { return };
        if last.is_named() || last.end_byte() != node.end_byte() {
            return;
        }
        let closer = last.kind();
        let is_closer = matches!(closer, "}" | ")" | "]" | "end" | "fi" | "done" | "esac" | ">") || closer.starts_with("</");
        if !is_closer || last.start_position().row != end {
            return;
        }
        // The line above the closer is the last hidden: `}` stays on screen. `foo {\n}` has nothing
        // inside to hide, which `insert_fold` sees for itself.
        self.insert_fold(start, end - 1);
    }

    fn insert_fold(&mut self, header: usize, last: usize) {
        if last > header {
            let e = self.folds.entry(header).or_insert(last);
            *e = (*e).max(last);
        }
    }

    pub fn fold_end(&self, line: usize) -> Option<usize> {
        self.folds.get(&line).copied()
    }

    /// Headers of the blocks holding `line`, outermost first.
    pub fn enclosing(&self, line: usize) -> Vec<usize> {
        let mut out: Vec<usize> = self.folds.range(..line).filter(|(_, end)| **end >= line).map(|(h, _)| *h).collect();
        out.sort_unstable();
        out
    }

    pub fn symbols(&self) -> &[Symbol] {
        &self.symbols
    }

    /// The smallest node that's bigger than the byte range.
    pub fn expand(&self, start: usize, end: usize) -> Option<(usize, usize)> {
        let tree = self.tree.as_ref()?;
        let mut node = tree.root_node().descendant_for_byte_range(start, end)?;
        loop {
            let (a, b) = (node.start_byte(), node.end_byte());
            if a <= start && end <= b && (a, b) != (start, end) {
                return Some((a, b));
            }
            node = node.parent()?;
        }
    }
}

fn text_of(node: Node<'_>, rope: &Rope) -> String {
    let (a, b) = (node.start_byte().min(rope.len_bytes()), node.end_byte().min(rope.len_bytes()));
    if a >= b {
        return String::new();
    }
    let s = rope.slice(rope.byte_to_char(a)..rope.byte_to_char(b)).to_string();
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// C-style names hide inside declarators: `*foo(int a)`.
fn declarator_name(node: Node<'_>, rope: &Rope) -> Option<String> {
    let mut n = node;
    for _ in 0..12 {
        match n.kind() {
            "identifier" | "field_identifier" | "type_identifier" | "destructor_name" | "operator_name" | "qualified_identifier" => {
                return Some(text_of(n, rope));
            }
            _ => {}
        }
        n = n.child_by_field_name("declarator").or_else(|| n.named_child(0))?;
    }
    None
}

fn symbol_of(node: Node<'_>, rope: &Rope) -> Option<(String, &'static str)> {
    let named = |field: &str| node.child_by_field_name(field).map(|n| text_of(n, rope));
    let (kind, name) = match node.kind() {
        // Rust
        "function_item" | "function_signature_item" => ("fn", named("name")),
        "struct_item" => ("struct", named("name")),
        "enum_item" => ("enum", named("name")),
        "union_item" => ("union", named("name")),
        "trait_item" => ("trait", named("name")),
        "mod_item" => ("mod", named("name")),
        "const_item" | "static_item" => ("const", named("name")),
        "type_item" => ("type", named("name")),
        "macro_definition" => ("macro", named("name")),
        "impl_item" => {
            let ty = named("type")?;
            let name = match named("trait") {
                Some(t) => format!("{t} for {ty}"),
                None => ty,
            };
            ("impl", Some(name))
        }
        // Python, C, C++, Bash
        "class_definition" => ("class", named("name")),
        "function_definition" => match node.child_by_field_name("declarator") {
            Some(d) => ("fn", declarator_name(d, rope)),
            None => ("fn", named("name")),
        },
        "struct_specifier" | "union_specifier" | "enum_specifier" | "class_specifier" => {
            // Only definitions: `struct foo x;` mentions a struct without defining one.
            node.child_by_field_name("body")?;
            ("type", named("name"))
        }
        "namespace_definition" => ("mod", named("name")),
        // JavaScript, TypeScript, Java, Go
        "function_declaration" | "generator_function_declaration" => ("fn", named("name")),
        "method_definition" | "method_declaration" | "constructor_declaration" => ("fn", named("name")),
        "class_declaration" | "abstract_class_declaration" => ("class", named("name")),
        "interface_declaration" | "enum_declaration" | "type_alias_declaration" | "record_declaration" => ("type", named("name")),
        "type_spec" => ("type", named("name")),
        "variable_declarator" => {
            let v = node.child_by_field_name("value")?;
            if !matches!(v.kind(), "arrow_function" | "function_expression" | "function") {
                return None;
            }
            ("fn", named("name"))
        }
        _ => return None,
    };
    let name = name.filter(|n| !n.is_empty())?;
    Some((name, kind))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::expand_selection;
    use crate::display::{Folds, enclosing_headers, fold_range, is_foldable};
    use crate::{Cursors, DisplayMap, Document, Edit, EditKind, Selection, language, outline};
    use std::time::{Duration, Instant};

    fn doc(lang: &str, src: &str) -> Document {
        let mut d = Document::new(src);
        d.language = language::by_name(lang);
        d.structure = Structure::new(lang);
        assert!(d.structure.is_some(), "{lang}");
        d.sync_structure();
        d
    }

    fn names(d: &Document) -> Vec<(String, &'static str, usize)> {
        outline::symbols(d).into_iter().map(|s| (s.name, s.kind, s.depth)).collect()
    }

    const RUST: &str = "mod m {\n    pub struct A {\n        x: u8,\n    }\n\n    impl Show for A {\n        fn run(&self) {\n            if true {\n                let s = \"}\";\n            }\n        }\n    }\n}\n";

    #[test]
    fn rust_folds_follow_the_syntax_not_the_indentation() {
        let d = doc("Rust", RUST);
        assert_eq!(fold_range(&d, 0, 4), Some(11), "mod up to the line before its brace");
        assert_eq!(fold_range(&d, 1, 4), Some(2));
        assert_eq!(fold_range(&d, 5, 4), Some(10));
        assert_eq!(fold_range(&d, 7, 4), Some(8));
        assert!(!is_foldable(&d, 2, 4));
        assert_eq!(enclosing_headers(&d, 8, 4, 9), vec![0, 5, 6, 7]);
        assert_eq!(enclosing_headers(&d, 8, 4, 2), vec![6, 7]);
    }

    #[test]
    fn badly_indented_code_still_folds_right() {
        let d = doc("C", "int f(void) {\nint a;\nif (a) {\nb();\n}\nreturn 0;\n}\n");
        assert_eq!(fold_range(&d, 0, 4), Some(5), "indentation alone couldn't tell");
        assert_eq!(fold_range(&d, 2, 4), Some(3));
    }

    #[test]
    fn python_blocks_fold_from_their_header() {
        let d = doc("Python", "class A:\n    def f(self):\n        return 1\n\n    def g(self):\n        x = 2\n        return x\n");
        assert_eq!(fold_range(&d, 0, 4), Some(6));
        assert_eq!(fold_range(&d, 4, 4), Some(6));
        assert_eq!(names(&d), vec![("A".into(), "class", 0), ("f".into(), "fn", 1), ("g".into(), "fn", 1)]);
    }

    #[test]
    fn outlines_have_nesting_and_ends() {
        let d = doc("Rust", RUST);
        assert_eq!(
            names(&d),
            vec![("m".into(), "mod", 0), ("A".into(), "struct", 1), ("Show for A".into(), "impl", 1), ("run".into(), "fn", 2)]
        );
        let s = outline::symbols(&d);
        assert_eq!((s[3].line, s[3].end), (6, Some(10)));
    }

    #[test]
    fn other_languages_list_their_symbols() {
        let check = |lang: &str, src: &str, want: &[&str]| {
            let got: Vec<String> = names(&doc(lang, src)).into_iter().map(|x| x.0).collect();
            assert_eq!(got, want, "{lang}");
        };
        check("C", "struct p { int x; };\nstruct p q;\nstatic int *soma(int a) {\n  return 0;\n}\n", &["p", "soma"]);
        check("C++", "namespace n {\nclass K {\n  void m() {}\n};\n}\n", &["n", "K", "m"]);
        check("JavaScript", "function a() {}\nconst b = () => 1;\nconst c = 2;\nclass D {\n  e() {}\n}\n", &["a", "b", "D", "e"]);
        check("TypeScript", "interface I { x: number }\ntype T = string;\nexport function f(): void {}\n", &["I", "T", "f"]);
        check("Go", "package p\nfunc f() {}\ntype T struct{}\nfunc (t T) m() {}\n", &["f", "T", "m"]);
        check("Java", "class A {\n  void b() {}\n}\n", &["A", "b"]);
        check("Bourne Again Shell (bash)", "f() {\n  echo\n}\n", &["f"]);
    }

    #[test]
    fn expanding_selection_climbs_the_tree() {
        let d = doc("Rust", "fn f() { g(a, b); }\n");
        let at = d.rope.to_string().find("a,").unwrap();
        let s = expand_selection(&d, Selection::cursor(at));
        assert_eq!(d.slice_string(s.start(), s.end()), "a");
        let s = expand_selection(&d, s);
        assert_eq!(d.slice_string(s.start(), s.end()), "(a, b)");
        let s = expand_selection(&d, s);
        assert_eq!(d.slice_string(s.start(), s.end()), "g(a, b)");
    }

    #[test]
    fn edits_keep_the_tree_in_step() {
        let mut d = doc("Rust", "fn a() {\n    x;\n}\n");
        let mut c = Cursors::default();
        assert_eq!(fold_range(&d, 0, 4), Some(1));
        // A non-ASCII char before the edit is where byte and char offsets part ways.
        let at = d.line_end(1);
        d.apply(vec![Edit::insert(at, "\n    ação;\n    y;\n    z;")], &mut c, EditKind::Other, |_, _| {});
        d.sync_structure();
        assert_eq!(fold_range(&d, 0, 4), Some(4));
        // Prepend a function: everything moves down.
        d.apply(vec![Edit::insert(0, "fn b() {}\n")], &mut c, EditKind::Other, |_, _| {});
        d.sync_structure();
        assert_eq!(fold_range(&d, 1, 4), Some(5));
        assert_eq!(names(&d).iter().map(|n| n.0.clone()).collect::<Vec<_>>(), vec!["b", "a"]);
        d.undo(&mut c);
        d.undo(&mut c);
        d.sync_structure();
        assert_eq!(names(&d).len(), 1);
        // A parse from scratch agrees with the incremental one.
        let fresh = doc("Rust", &d.rope.to_string());
        assert_eq!(fold_range(&fresh, 0, 4), fold_range(&d, 0, 4));
    }

    #[test]
    fn broken_code_still_gets_a_tree() {
        let d = doc("Rust", "fn a( {\n    let x = ;\n    let y = ;\n    let z = ;\n}\nfn ok() {\n    1;\n    2;\n}\n");
        assert!(names(&d).iter().any(|n| n.0 == "ok"));
    }

    #[test]
    fn folds_work_with_the_display_map() {
        let d = doc("Rust", "fn a() {\n    x;\n    y;\n}\nfn b() {}\n");
        let mut f = Folds::default();
        f.add(0, fold_range(&d, 0, 4).unwrap());
        let m = DisplayMap::new(&d, &f, None, 4);
        assert_eq!(m.row(&d, 1).line, 3, "the closing brace stays visible");
    }

    #[test]
    fn typing_in_a_big_file_is_quick() {
        let body = "    let value = compute(alpha, beta);\n".repeat(20);
        let mut src = String::new();
        for i in 0..2000 {
            src.push_str(&format!("fn f{i}() {{\n{body}}}\n"));
        }
        let mut d = doc("Rust", &src);
        let mut c = Cursors::default();
        let at = d.pos(20_000, 4);
        let t = Instant::now();
        for i in 0..20 {
            d.apply(vec![Edit::insert(at + i, "x")], &mut c, EditKind::Typing, |_, _| {});
            d.sync_structure();
        }
        let each = t.elapsed() / 20;
        eprintln!("{} lines: {each:?} per edit", d.len_lines());
        assert!(each < Duration::from_millis(100), "{each:?}");
    }

    #[test]
    fn languages_without_a_parser_just_say_so() {
        assert!(Structure::new("Plain Text").is_none());
        assert!(Structure::new("Lua").is_none());
    }
}
