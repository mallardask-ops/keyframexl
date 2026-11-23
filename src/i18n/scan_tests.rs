//! Fails on a new English string literal in `src/ui`: user-visible text
//! belongs in the `Strings` tables. Literals that are not UI text (ids,
//! keys, units, brand names, default data names) are listed in
//! `untranslated.txt` next to this file; the list must also stay current.

use std::collections::BTreeSet;
use std::path::Path;
use syn::visit::{self, Visit};

/// `shortcuts.rs` carries its own English and Ukrainian tables (checked by
/// its tests); the `*tests.rs` files are test code.
fn skipped_file(name: &str) -> bool {
    name == "shortcuts.rs" || name.ends_with("tests.rs")
}

/// Methods and calls whose string arguments are ids, keys, paths or patterns.
const SKIP_METHODS: &[&str] = &[
    "with",
    "id_salt",
    "push_id",
    "from_id_salt",
    "join",
    "starts_with",
    "ends_with",
    "contains",
    "strip_prefix",
    "strip_suffix",
    "split",
    "split_once",
    "rsplit_once",
    "replace",
    "find",
    "trim_start_matches",
    "trim_end_matches",
    "with_extension",
    "set_file_name",
    "arg",
    "args",
    "env",
    "get",
    "remove",
    "insert",
    "expect",
];
const SKIP_CALLS: &[&str] = &[
    "Id::new",
    "env::var",
    "env::var_os",
    "std::env::var",
    "std::env::var_os",
    "Command::new",
    "std::process::Command::new",
    "Path::new",
    "std::path::Path::new",
    "PathBuf::from",
];
/// Macros whose strings never reach the interface.
const SKIP_MACROS: &[&str] = &[
    "eprintln",
    "println",
    "eprint",
    "print",
    "panic",
    "assert",
    "assert_eq",
    "assert_ne",
    "debug_assert",
    "debug_assert_eq",
    "unreachable",
    "todo",
    "include_str",
    "include_bytes",
    "concat",
    "env",
    "matches",
];

struct Finder {
    found: Vec<String>,
}

fn is_cfg_test(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|a| {
        a.path().is_ident("cfg")
            && a.parse_args::<syn::Meta>()
                .is_ok_and(|m| m.path().is_ident("test"))
    })
}

fn path_text(p: &syn::Path) -> String {
    p.segments
        .iter()
        .map(|s| s.ident.to_string())
        .collect::<Vec<_>>()
        .join("::")
}

impl Finder {
    fn push(&mut self, s: String) {
        self.found.push(s);
    }

    /// String literals among a macro's tokens (format!, write!, vec!, …).
    fn tokens(&mut self, ts: proc_macro2::TokenStream) {
        for tt in ts {
            match tt {
                proc_macro2::TokenTree::Literal(l) => {
                    if let Ok(syn::Lit::Str(s)) = syn::parse_str::<syn::Lit>(&l.to_string()) {
                        self.push(s.value());
                    }
                }
                proc_macro2::TokenTree::Group(g) => self.tokens(g.stream()),
                _ => {}
            }
        }
    }
}

impl<'ast> Visit<'ast> for Finder {
    fn visit_attribute(&mut self, _: &'ast syn::Attribute) {}

    fn visit_item(&mut self, i: &'ast syn::Item) {
        let attrs = match i {
            syn::Item::Mod(m) => &m.attrs,
            syn::Item::Fn(f) => &f.attrs,
            syn::Item::Impl(m) => &m.attrs,
            syn::Item::Const(c) => &c.attrs,
            syn::Item::Static(s) => &s.attrs,
            syn::Item::Struct(s) => &s.attrs,
            syn::Item::Enum(e) => &e.attrs,
            syn::Item::Use(u) => &u.attrs,
            _ => return visit::visit_item(self, i),
        };
        if !is_cfg_test(attrs) {
            visit::visit_item(self, i);
        }
    }

    fn visit_impl_item_fn(&mut self, f: &'ast syn::ImplItemFn) {
        if !is_cfg_test(&f.attrs) {
            visit::visit_impl_item_fn(self, f);
        }
    }

    fn visit_stmt(&mut self, s: &'ast syn::Stmt) {
        let attrs = match s {
            syn::Stmt::Local(l) => &l.attrs,
            syn::Stmt::Macro(m) => &m.attrs,
            _ => return visit::visit_stmt(self, s),
        };
        if !is_cfg_test(attrs) {
            visit::visit_stmt(self, s);
        }
    }

    fn visit_expr(&mut self, e: &'ast syn::Expr) {
        let attrs: &[syn::Attribute] = match e {
            syn::Expr::Block(b) => &b.attrs,
            syn::Expr::If(b) => &b.attrs,
            syn::Expr::Call(b) => &b.attrs,
            syn::Expr::MethodCall(b) => &b.attrs,
            _ => &[],
        };
        if !is_cfg_test(attrs) {
            visit::visit_expr(self, e);
        }
    }

    fn visit_pat(&mut self, _: &'ast syn::Pat) {}

    fn visit_expr_binary(&mut self, b: &'ast syn::ExprBinary) {
        if matches!(b.op, syn::BinOp::Eq(_) | syn::BinOp::Ne(_)) {
            return;
        }
        visit::visit_expr_binary(self, b);
    }

    fn visit_expr_method_call(&mut self, m: &'ast syn::ExprMethodCall) {
        self.visit_expr(&m.receiver);
        if !SKIP_METHODS.contains(&m.method.to_string().as_str()) {
            for a in &m.args {
                self.visit_expr(a);
            }
        }
    }

    fn visit_expr_call(&mut self, c: &'ast syn::ExprCall) {
        if let syn::Expr::Path(p) = &*c.func {
            let name = path_text(&p.path);
            if SKIP_CALLS.iter().any(|s| name.ends_with(s)) {
                return;
            }
        }
        visit::visit_expr_call(self, c);
    }

    fn visit_macro(&mut self, m: &'ast syn::Macro) {
        let name = m
            .path
            .segments
            .last()
            .map(|s| s.ident.to_string())
            .unwrap_or_default();
        if !SKIP_MACROS.contains(&name.as_str()) {
            self.tokens(m.tokens.clone());
        }
    }

    fn visit_lit_str(&mut self, s: &'ast syn::LitStr) {
        self.push(s.value());
    }
}

/// Looks like words a person reads (not an id, key, path, unit or symbol).
fn looks_like_text(s: &str) -> bool {
    let mut bare = String::new();
    let mut depth = 0;
    for c in s.chars() {
        match c {
            '{' => depth += 1,
            '}' => depth -= 1,
            _ if depth == 0 => bare.push(c),
            _ => {}
        }
    }
    let has_word = bare
        .split(|c: char| !c.is_ascii_alphabetic())
        .any(|w| w.len() >= 3);
    if !has_word {
        return false;
    }
    let id_like = s
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || "_-:./{}".contains(c));
    let const_like = s
        .chars()
        .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_');
    !(id_like || const_like)
}

fn collect_rs(root: &Path, dir: &Path, out: &mut Vec<(String, std::path::PathBuf)>) {
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .expect("src/ui")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            collect_rs(root, &path, out);
        } else if path.extension().is_some_and(|x| x == "rs") {
            let rel = path
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            out.push((rel, path));
        }
    }
}

/// `path: text` lines for every suspicious literal under `src/ui`.
fn scan() -> BTreeSet<String> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/ui");
    let mut out = BTreeSet::new();
    let mut files = Vec::new();
    collect_rs(&dir, &dir, &mut files);
    for (name, path) in files {
        let base = path.file_name().unwrap().to_string_lossy();
        if skipped_file(&base) {
            continue;
        }
        let src = std::fs::read_to_string(&path).unwrap();
        let file = syn::parse_file(&src).unwrap_or_else(|e| panic!("{name}: {e}"));
        let mut f = Finder { found: vec![] };
        f.visit_file(&file);
        for s in f.found {
            if looks_like_text(&s) {
                out.insert(format!("{name}: {}", s.replace('\n', "\\n")));
            }
        }
    }
    out
}

fn allowlist() -> BTreeSet<String> {
    include_str!("untranslated.txt")
        .lines()
        .filter(|l| !l.trim().is_empty() && !l.starts_with('#'))
        .map(str::to_owned)
        .collect()
}

#[test]
fn ui_has_no_new_english_literals() {
    let found = scan();
    let allowed = allowlist();
    let new: Vec<_> = found.difference(&allowed).collect();
    assert!(
        new.is_empty(),
        "English text in src/ui: move it to src/i18n (en.rs and uk.rs), or, if it is \
         not UI text, add the line to src/i18n/untranslated.txt:\n{}",
        new.iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    );
    let stale: Vec<_> = allowed.difference(&found).collect();
    assert!(
        stale.is_empty(),
        "src/i18n/untranslated.txt lists literals that are gone; remove these lines:\n{}",
        stale
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    );
}

#[test]
fn the_heuristic_tells_text_from_ids() {
    assert!(looks_like_text("Save changes?"));
    assert!(looks_like_text("{n} notes selected"));
    assert!(!looks_like_text("tide-tbl-{id}"));
    assert!(!looks_like_text("KFXL_CAPTURE"));
    assert!(!looks_like_text("{:.1} ms"));
    assert!(!looks_like_text("▶"));
}
