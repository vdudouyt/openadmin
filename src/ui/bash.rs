//! Bash, coloured for reading before it runs.
//!
//! The plan dialog is where an operator decides whether a script runs on a
//! fleet, so this lexer is held to one rule above looking right: **it never
//! makes live code look inert.** Three invariants follow, and the tests are
//! organised around them:
//!
//! - **I1.** A character is `Comment` only where bash itself discards it: at a
//!   token boundary of the script's top level. Not "after whitespace" — `a\ #b`
//!   and a `\`-continued `safe\⏎#` are live code — and not "err toward missing
//!   comments" either: a missed comment's text is read as code, and the `'` in
//!   `nginx;# don't wait` would open a quote that paints the next line, live,
//!   as a string. A `#` at a boundary anywhere else gives up.
//! - **I2.** Colour only. What is shown is decided by `widgets::reveal_indexed`;
//!   this module only says what each raw byte is.
//! - **I3.** Lost means plain. An unterminated quote, a delimiter it cannot
//!   read, anything it does not model: from the outermost open construct to the
//!   end of the script, everything is `Plain` — never `Comment`, never `Quoted`.
//!
//! It reads the *raw* script, not what is displayed: an escape sequence hidden
//! from the screen still makes the `#` after it part of a word, as bash sees.
//! It is a lexer that is right about the dangerous case and modest elsewhere —
//! command position, which only decides emphasis, is a good guess.

use crate::ui::theme;
use crate::ui::widgets::{reveal_indexed, wrap_exact};
use ratatui::style::Style;
use ratatui::text::Span;

/// What a byte of the script is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Class {
    Plain,
    /// The first word of a command: what runs.
    Command,
    Keyword,
    Operator,
    /// Inside quotes. Not `String`, to keep clear of the type.
    Quoted,
    Expansion,
    Comment,
}

/// A class for every byte of `script`; a multi-byte character's bytes share
/// its class.
pub fn classify(script: &str) -> Vec<Class> {
    let mut lx = Lexer::new(script);
    lx.run();
    lx.out
}

/// One row of a script as the plan dialog draws it.
pub struct ScriptRow {
    /// The second and later rows of a line too long for one: drawn under a
    /// different gutter, so one wrapped command cannot read as two.
    pub continuation: bool,
    pub spans: Vec<Span<'static>>,
}

/// The rows of `script` at `width` cells: every character bash reads shown —
/// control characters and invisible Unicode by name, in the alarm style — each
/// coloured by what bash makes of the raw byte behind it.
pub fn script_rows(script: &str, width: usize) -> Vec<ScriptRow> {
    let classes = classify(script);
    let mut rows = Vec::new();
    let mut off = 0;
    // `split_inclusive` has the lines `lines` has, and keeps count of bytes.
    // A `\r` before the `\n` stays in the line and is shown: bash reads it.
    for piece in script.split_inclusive('\n') {
        let line = piece.strip_suffix('\n').unwrap_or(piece);
        let cells = reveal_indexed(line);
        let chars: Vec<char> = cells.iter().map(|(c, _, _)| *c).collect();
        let styles: Vec<Style> = cells
            .iter()
            .map(|&(_, at, hidden)| {
                if hidden {
                    theme::script_hidden()
                } else {
                    style(classes[off + at])
                }
            })
            .collect();
        for (k, r) in wrap_exact(&chars, width).into_iter().enumerate() {
            rows.push(ScriptRow {
                continuation: k > 0,
                spans: runs(&chars[r.clone()], &styles[r]),
            });
        }
        off += piece.len();
    }
    rows
}

fn style(c: Class) -> Style {
    match c {
        Class::Plain => theme::body(),
        Class::Command => theme::script_command(),
        Class::Keyword => theme::script_keyword(),
        Class::Operator => theme::script_operator(),
        Class::Quoted => theme::script_string(),
        Class::Expansion => theme::script_expansion(),
        Class::Comment => theme::script_comment(),
    }
}

/// Runs of one style, as one span each.
fn runs(chars: &[char], styles: &[Style]) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let mut j = i;
        while j < chars.len() && styles[j] == styles[i] {
            j += 1;
        }
        spans.push(Span::styled(
            chars[i..j].iter().collect::<String>(),
            styles[i],
        ));
        i = j;
    }
    spans
}

// ---- the lexer ---------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    /// Commands: the script itself (`top`), or `$(…)`, `<(…)`, `>(…)`.
    Code,
    /// `[[ … ]]`.
    Cond,
    /// `$((…))`, or `((…))` as a command.
    Arith,
    /// Words bash reads whole: extglob `@(…)`, `name=(…)`, `name[…]`, and a
    /// `(` inside `[[`. Plain, and a `#` at a boundary in one gives up.
    Group,
    Single,
    /// `$'…'`.
    Ansi,
    Double,
    /// `${…}`.
    Brace,
    /// `` `…` `` — opaque: bash finds its end without regard to quotes.
    Backtick,
}

/// Where a `case` is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CaseAt {
    Subject,
    WantIn,
    Pattern,
    Body,
}

/// What the next word is, when a keyword says.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Expect {
    Nothing,
    /// After `for` or `select`: the loop's variable.
    LoopName,
    /// After the loop's variable: `in`, maybe.
    LoopIn,
    /// After `function`.
    FuncName,
    /// After a redirection: its target.
    Target,
}

/// The word being read in a code context. Its own characters are coloured when
/// it ends, once it is known whether it is a keyword, a command or an argument.
#[derive(Clone, Debug)]
struct Word {
    /// Byte offset of its first character.
    start: usize,
    /// Characters the code context read itself, by byte offset — not those of
    /// quotes or expansions inside it, which colour themselves.
    own: Vec<usize>,
    /// Its text up to the first quote or expansion in it.
    prefix: String,
    /// Whether a quote or expansion is part of it.
    nested: bool,
}

#[derive(Clone, Debug)]
struct Ctx {
    kind: Kind,
    /// Byte offset of what opened it — where I3 turns plain from.
    opener: usize,
    /// Code: the script itself, rather than a substitution.
    top: bool,
    /// Arith: `$((` rather than the `((` command. Double and Brace: inside
    /// double quotes.
    flag: bool,
    /// Group: its closing character.
    close: char,
    /// Nesting of `(` inside Code, Arith and Group.
    depth: usize,
    /// At a token boundary, as bash would be: what decides a `#`.
    boundary: bool,
    // Code only.
    cmd_pos: bool,
    word: Option<Word>,
    expect: Expect,
    case: Vec<CaseAt>,
    /// The `(` of `name()` has just been read.
    after_open_paren: bool,
    // Cond only: the word being read, to find `]]`.
    cond_word: String,
}

impl Ctx {
    fn new(kind: Kind, opener: usize) -> Self {
        Ctx {
            kind,
            opener,
            top: false,
            flag: false,
            close: ')',
            depth: 0,
            boundary: true,
            cmd_pos: true,
            word: None,
            expect: Expect::Nothing,
            case: Vec::new(),
            after_open_paren: false,
            cond_word: String::new(),
        }
    }
}

#[derive(Clone, Debug)]
struct Heredoc {
    delim: String,
    strip_tabs: bool,
    /// An unquoted delimiter: a body line ending in an odd run of `\` joins the
    /// next.
    joins: bool,
}

struct Lexer {
    /// Characters with their byte offsets.
    cs: Vec<(usize, char)>,
    len: usize,
    i: usize,
    out: Vec<Class>,
    stack: Vec<Ctx>,
    heredocs: Vec<Heredoc>,
    done: bool,
}

fn is_meta(c: char) -> bool {
    matches!(
        c,
        ' ' | '\t' | '\n' | ';' | '&' | '|' | '(' | ')' | '<' | '>'
    )
}

fn is_name_start(c: char) -> bool {
    c == '_' || c.is_ascii_alphabetic()
}

fn is_name_char(c: char) -> bool {
    c == '_' || c.is_ascii_alphanumeric()
}

fn is_name(s: &str) -> bool {
    let mut cs = s.chars();
    cs.next().is_some_and(is_name_start) && cs.all(is_name_char)
}

/// `NAME=`, `NAME+=`, `NAME[…]=`: the length of the name, when `prefix` starts
/// an assignment.
fn assignment_name(prefix: &str) -> Option<usize> {
    let eq = prefix.find('=')?;
    let lhs = prefix[..eq].strip_suffix('+').unwrap_or(&prefix[..eq]);
    let name = match lhs.find('[') {
        Some(b) if lhs.ends_with(']') => &lhs[..b],
        Some(_) => return None,
        None => lhs,
    };
    is_name(name).then_some(name.len())
}

/// Words that change how later lines are read — an alias to `'` opens a quote
/// this lexer cannot see. Cheap defence, not a proof.
fn changes_the_reading(word: &str) -> bool {
    word == "alias"
        || word.contains("expand_aliases")
        || word.contains("BASH_ALIASES")
        || word.contains("histexpand")
}

impl Lexer {
    fn new(src: &str) -> Self {
        let mut top = Ctx::new(Kind::Code, 0);
        top.top = true;
        Lexer {
            cs: src.char_indices().collect(),
            len: src.len(),
            i: 0,
            out: vec![Class::Plain; src.len()],
            stack: vec![top],
            heredocs: Vec::new(),
            done: false,
        }
    }

    // ---- positions --------------------------------------------------------

    fn at(&self, i: usize) -> Option<char> {
        self.cs.get(i).map(|(_, c)| *c)
    }

    fn byte(&self, i: usize) -> usize {
        self.cs.get(i).map_or(self.len, |(b, _)| *b)
    }

    fn is_continuation(&self, i: usize) -> bool {
        self.at(i) == Some('\\') && self.at(i + 1) == Some('\n')
    }

    /// The `k`th character from `i`, not counting `\⏎` pairs — what bash sees
    /// wherever it removes them. Returns its index.
    fn logical(&self, mut i: usize, k: usize) -> Option<usize> {
        let mut n = 0;
        loop {
            while self.is_continuation(i) {
                i += 2;
            }
            self.at(i)?;
            if n == k {
                return Some(i);
            }
            n += 1;
            i += 1;
        }
    }

    fn la(&self, k: usize) -> Option<char> {
        self.logical(self.i, k).and_then(|i| self.at(i))
    }

    fn mark(&mut self, i: usize, class: Class) {
        // I3: once given up, nothing more is vouched for.
        if self.done {
            return;
        }
        if let Some(&(b, c)) = self.cs.get(i) {
            for x in b..b + c.len_utf8() {
                self.out[x] = class;
            }
        }
    }

    /// Consume one logical character, colouring it and any `\⏎` before it.
    fn take(&mut self, class: Class) -> Option<char> {
        while self.is_continuation(self.i) {
            self.mark(self.i, class);
            self.mark(self.i + 1, class);
            self.i += 2;
        }
        let c = self.at(self.i)?;
        self.mark(self.i, class);
        self.i += 1;
        Some(c)
    }

    /// I3: from `from` on, nothing is vouched for.
    fn give_up(&mut self, from: usize) {
        let from = self.stack[1..]
            .iter()
            .map(|c| c.opener)
            .chain([from])
            .min()
            .unwrap_or(from);
        for b in from..self.len {
            self.out[b] = Class::Plain;
        }
        self.done = true;
    }

    fn top(&mut self) -> &mut Ctx {
        self.stack
            .last_mut()
            .expect("the script's own context is never popped")
    }

    fn push(&mut self, kind: Kind, opener: usize) -> &mut Ctx {
        // A construct opened inside a word is part of it.
        if let Some(parent) = self.stack.last_mut() {
            parent.boundary = false;
            if parent.kind == Kind::Code {
                let w = parent.word.get_or_insert_with(|| Word {
                    start: opener,
                    own: Vec::new(),
                    prefix: String::new(),
                    nested: false,
                });
                w.nested = true;
            }
        }
        self.stack.push(Ctx::new(kind, opener));
        self.stack.last_mut().expect("just pushed")
    }

    fn pop(&mut self) {
        if self.stack.len() > 1 {
            self.stack.pop();
        }
    }

    // ---- the loop ---------------------------------------------------------

    fn run(&mut self) {
        while !self.done && self.i < self.cs.len() {
            if self.at(self.i) == Some('\0') {
                // bash 5.2 drops NULs before it reads a word; what it makes of
                // the rest is not this lexer's to guess.
                let b = self.byte(self.i);
                self.give_up(b);
                return;
            }
            let kind = self.stack.last().map(|c| c.kind).unwrap_or(Kind::Code);
            match kind {
                Kind::Code => self.code(),
                Kind::Cond => self.cond(),
                Kind::Arith => self.arith(),
                Kind::Group => self.group(),
                Kind::Single => self.single(),
                Kind::Ansi => self.ansi(),
                Kind::Double => self.double(),
                Kind::Brace => self.brace(),
                Kind::Backtick => self.backtick(),
            }
        }
        if self.done {
            return;
        }
        if self.stack.len() > 1 {
            // bash reports an unexpected end and runs none of it.
            let b = self.stack[1].opener;
            self.give_up(b);
            return;
        }
        self.end_word();
    }

    /// A `\⏎` where bash removes it: consumed, changing nothing. Returns
    /// whether there was one.
    fn continuation(&mut self, class: Class) -> bool {
        if self.is_continuation(self.i) {
            self.mark(self.i, class);
            self.mark(self.i + 1, class);
            self.i += 2;
            true
        } else {
            false
        }
    }

    /// A newline outside the top level while heredocs wait for their bodies:
    /// bash's handling differs by construct and version. Returns whether it
    /// gave up.
    fn newline_with_heredocs_pending(&mut self) -> bool {
        if self.at(self.i) == Some('\n') && !self.heredocs.is_empty() {
            let b = self.byte(self.i);
            self.give_up(b);
            true
        } else {
            false
        }
    }

    /// `$…` wherever it expands. The caller has checked for `$`. `in_double`:
    /// `$'` and `$"` are literal there.
    fn dollar(&mut self, in_double: bool, literal: Class) {
        let at = self.byte(self.i);
        match (self.la(1), self.la(2)) {
            (Some('('), Some('(')) => {
                for _ in 0..3 {
                    self.take(Class::Expansion);
                }
                let c = self.push(Kind::Arith, at);
                c.flag = true;
            }
            (Some('('), _) => {
                self.take(Class::Operator);
                self.take(Class::Operator);
                self.push(Kind::Code, at);
            }
            (Some('{'), _) => {
                self.take(Class::Expansion);
                self.take(Class::Expansion);
                self.push(Kind::Brace, at).flag = in_double;
            }
            (Some('\''), _) if !in_double => {
                self.take(Class::Quoted);
                self.take(Class::Quoted);
                self.push(Kind::Ansi, at);
            }
            (Some('"'), _) if !in_double => {
                self.take(Class::Quoted);
                self.take(Class::Quoted);
                self.push(Kind::Double, at).flag = true;
            }
            (Some('['), _) => self.give_up(at),
            (Some(c), _) if is_name_start(c) => {
                self.take(Class::Expansion);
                while self.la(0).is_some_and(is_name_char) {
                    self.take(Class::Expansion);
                }
                self.after_expansion(at);
            }
            (Some(c), _) if c.is_ascii_digit() || "@*#?$!-".contains(c) => {
                self.take(Class::Expansion);
                self.take(Class::Expansion);
                self.after_expansion(at);
            }
            _ => {
                // A `$` that expands nothing is itself.
                self.take(literal);
                if self.stack.last().map(|c| c.kind) == Some(Kind::Code) {
                    self.own_char();
                } else {
                    self.top().boundary = false;
                }
            }
        }
    }

    /// A `$NAME` inside a word: part of it, and no longer at a boundary.
    fn after_expansion(&mut self, at: usize) {
        let c = self.top();
        c.boundary = false;
        if c.kind == Kind::Code {
            let w = c.word.get_or_insert_with(|| Word {
                start: at,
                own: Vec::new(),
                prefix: String::new(),
                nested: false,
            });
            w.nested = true;
        }
    }

    /// Record the character just taken as the current word's own.
    fn own_char(&mut self) {
        let i = self.i - 1;
        let (b, ch) = self.cs[i];
        let c = self.top();
        c.boundary = false;
        c.after_open_paren = false;
        let w = c.word.get_or_insert_with(|| Word {
            start: b,
            own: Vec::new(),
            prefix: String::new(),
            nested: false,
        });
        w.own.push(i);
        if !w.nested {
            w.prefix.push(ch);
        }
    }

    // ---- code -----------------------------------------------------------------

    fn code(&mut self) {
        if self.continuation(Class::Plain) {
            return;
        }
        let Some(c) = self.at(self.i) else { return };
        let top = self.stack.last().is_some_and(|c| c.top);
        if c == '\n' && !top && self.newline_with_heredocs_pending() {
            return;
        }
        match c {
            '#' if self.top().boundary => {
                if top {
                    // I1: what bash discards, to the end of the line — a `\`
                    // at its end continues nothing.
                    while self.at(self.i).is_some_and(|c| c != '\n') {
                        self.mark(self.i, Class::Comment);
                        self.i += 1;
                    }
                } else {
                    let b = self.byte(self.i);
                    self.give_up(b);
                }
            }
            ' ' | '\t' => {
                self.end_word();
                self.take(Class::Plain);
                self.top().boundary = true;
            }
            '\n' => {
                self.end_word();
                self.take(Class::Plain);
                let t = self.top();
                t.boundary = true;
                t.cmd_pos = true;
                t.after_open_paren = false;
                if matches!(t.expect, Expect::LoopIn | Expect::Target) {
                    t.expect = Expect::Nothing;
                }
                if top && !self.heredocs.is_empty() {
                    self.heredoc_bodies();
                }
            }
            ';' => {
                self.end_word();
                self.take(Class::Operator);
                let case_sep = self.la(0) == Some(';');
                let amp = self.la(0) == Some('&');
                if case_sep {
                    self.take(Class::Operator);
                    if self.la(0) == Some('&') {
                        self.take(Class::Operator);
                    }
                } else if amp {
                    self.take(Class::Operator);
                }
                let t = self.top();
                t.boundary = true;
                t.expect = Expect::Nothing;
                if (case_sep || amp) && t.case.last() == Some(&CaseAt::Body) {
                    *t.case.last_mut().expect("checked") = CaseAt::Pattern;
                    t.cmd_pos = false;
                } else {
                    t.cmd_pos = true;
                }
            }
            '&' => {
                self.end_word();
                self.take(Class::Operator);
                match self.la(0) {
                    Some('&') => {
                        self.take(Class::Operator);
                        self.top().cmd_pos = true;
                    }
                    Some('>') => {
                        self.take(Class::Operator);
                        if self.la(0) == Some('>') {
                            self.take(Class::Operator);
                        }
                        self.top().expect = Expect::Target;
                    }
                    _ => self.top().cmd_pos = true,
                }
                self.top().boundary = true;
            }
            '|' => {
                self.end_word();
                self.take(Class::Operator);
                if matches!(self.la(0), Some('|' | '&')) {
                    self.take(Class::Operator);
                }
                let t = self.top();
                t.boundary = true;
                if t.case.last() != Some(&CaseAt::Pattern) {
                    t.cmd_pos = true;
                }
            }
            '(' => self.code_open_paren(),
            ')' => self.code_close_paren(),
            '<' | '>' => self.redirection(c),
            '\\' => {
                // An escaped character is a word character.
                self.take(Class::Plain);
                self.own_char();
                if self.at(self.i).is_some() {
                    self.take(Class::Plain);
                    self.own_char();
                }
            }
            '\'' => {
                let b = self.byte(self.i);
                self.take(Class::Quoted);
                self.push(Kind::Single, b);
            }
            '"' => {
                let b = self.byte(self.i);
                self.take(Class::Quoted);
                self.push(Kind::Double, b).flag = true;
            }
            '`' => {
                let b = self.byte(self.i);
                self.take(Class::Expansion);
                self.push(Kind::Backtick, b);
            }
            '$' => self.dollar(false, Class::Plain),
            _ => {
                // `name[` in command position: a subscript, read whole.
                let subscript = c == '['
                    && self.top().cmd_pos
                    && self
                        .top()
                        .word
                        .as_ref()
                        .is_some_and(|w| !w.nested && is_name(&w.prefix));
                self.take(Class::Plain);
                self.own_char();
                if subscript {
                    let b = self.byte(self.i - 1);
                    let g = self.push_group(']', b);
                    g.boundary = false;
                }
            }
        }
    }

    fn push_group(&mut self, close: char, at: usize) -> &mut Ctx {
        let g = self.push(Kind::Group, at);
        g.close = close;
        g
    }

    fn code_open_paren(&mut self) {
        let at = self.byte(self.i);
        let mid_word = self.top().word.is_some();
        if mid_word {
            let (extglob, assignment) = {
                let w = self.top().word.as_ref().expect("mid-word");
                let last = w.prefix.chars().last();
                let extglob = !w.nested && matches!(last, Some('@' | '*' | '+' | '?' | '!'));
                let assignment =
                    !w.nested && w.prefix.ends_with('=') && assignment_name(&w.prefix).is_some();
                (extglob, assignment)
            };
            if extglob || assignment {
                // `@(…)` and `name=(…)` are read whole, as one word.
                self.take(Class::Plain);
                self.own_char();
                let g = self.push_group(')', at);
                g.boundary = true;
                return;
            }
        }
        // Otherwise `(` ends a word and starts something.
        self.end_word();
        let cmd_pos = self.top().cmd_pos;
        let for_head = self.top().expect == Expect::LoopName;
        let in_pattern = self.top().case.last() == Some(&CaseAt::Pattern);
        if (cmd_pos || for_head) && !in_pattern && self.la(1) == Some('(') {
            // `((…))`, as a command or as `for ((…))`: arithmetic, where `<<`
            // is a shift and not a heredoc.
            self.top().expect = Expect::Nothing;
            self.take(Class::Keyword);
            self.take(Class::Keyword);
            self.push(Kind::Arith, at);
            return;
        }
        self.take(Class::Operator);
        let t = self.top();
        t.boundary = true;
        if in_pattern {
            return;
        }
        t.depth += 1;
        t.cmd_pos = true;
        t.after_open_paren = true;
    }

    fn code_close_paren(&mut self) {
        self.end_word();
        if self.done {
            return;
        }
        let at = self.byte(self.i);
        let (in_pattern, depth, function, top) = {
            let t = self.stack.last().expect("code context");
            (
                t.case.last() == Some(&CaseAt::Pattern),
                t.depth,
                t.after_open_paren,
                t.top,
            )
        };
        if in_pattern && depth == 0 {
            self.take(Class::Operator);
            let t = self.top();
            *t.case.last_mut().expect("checked") = CaseAt::Body;
            t.cmd_pos = true;
            t.boundary = true;
            return;
        }
        if depth > 0 {
            self.take(Class::Operator);
            let t = self.top();
            t.depth -= 1;
            t.boundary = true;
            // `name()`: a function's body comes next.
            t.cmd_pos = function;
            t.after_open_paren = false;
            return;
        }
        if !top {
            // The end of `$(…)`, `<(…)` or `>(…)`: part of the word it is in.
            self.take(Class::Operator);
            self.pop();
            self.top().boundary = false;
            return;
        }
        // A `)` with nothing to close: bash will not run this.
        self.give_up(at);
    }

    fn redirection(&mut self, c: char) {
        let at = self.byte(self.i);
        // Digits right before `<` or `>` are the redirection's file descriptor.
        let fd = self.top().word.as_ref().is_some_and(|w| {
            !w.nested && !w.prefix.is_empty() && w.prefix.chars().all(|c| c.is_ascii_digit())
        });
        if fd {
            let w = self.top().word.take().expect("checked");
            for i in w.own {
                self.mark(i, Class::Operator);
            }
        } else {
            self.end_word();
        }
        // `<(` and `>(`: process substitution.
        if self.la(1) == Some('(') {
            self.take(Class::Operator);
            self.take(Class::Operator);
            self.push(Kind::Code, at);
            return;
        }
        self.take(Class::Operator);
        let top = self.stack.last().is_some_and(|c| c.top);
        match (c, self.la(0), self.la(1)) {
            ('<', Some('<'), Some('<')) => {
                self.take(Class::Operator);
                self.take(Class::Operator);
            }
            ('<', Some('<'), _) => {
                if !top {
                    // bash versions read a heredoc inside `$(…)` differently.
                    self.give_up(at);
                    return;
                }
                self.take(Class::Operator);
                self.heredoc_operator(at);
                return;
            }
            ('<', Some('&' | '>'), _) | ('>', Some('>' | '&' | '|'), _) => {
                self.take(Class::Operator);
            }
            _ => {}
        }
        let t = self.top();
        t.boundary = true;
        t.expect = Expect::Target;
    }

    /// After `<<`: the optional `-`, then the delimiter. Anything this does not
    /// read exactly gives up.
    fn heredoc_operator(&mut self, at: usize) {
        let strip_tabs = self.la(0) == Some('-');
        if strip_tabs {
            self.take(Class::Operator);
        }
        while matches!(self.la(0), Some(' ' | '\t')) {
            self.take(Class::Plain);
        }
        let quote = match self.la(0) {
            Some(q @ ('\'' | '"')) => Some(q),
            Some('\\') => Some('\\'),
            _ => None,
        };
        if quote.is_some() {
            self.take(Class::Operator);
        }
        let word_char = |c: char| is_name_char(c) || c == '.' || c == '-';
        let mut delim = String::new();
        while let Some(c) = self.la(0).filter(|c| word_char(*c)) {
            delim.push(c);
            self.take(Class::Operator);
        }
        if let Some(q @ ('\'' | '"')) = quote {
            if self.la(0) != Some(q) {
                self.give_up(at);
                return;
            }
            self.take(Class::Operator);
        }
        let ends = self.la(0).is_none_or(is_meta);
        if delim.is_empty() || !ends {
            self.give_up(at);
            return;
        }
        self.heredocs.push(Heredoc {
            delim,
            strip_tabs,
            joins: quote.is_none(),
        });
        let t = self.top();
        t.boundary = true;
    }

    /// The bodies of the heredocs waiting, in order, from the start of the line
    /// after their operators.
    fn heredoc_bodies(&mut self) {
        let pending = std::mem::take(&mut self.heredocs);
        for hd in pending {
            loop {
                if self.i >= self.cs.len() {
                    // No terminator: bash runs it, warning; the body is plain.
                    return;
                }
                // One logical line: an unquoted body joins a line ending in an
                // odd run of `\` with the next.
                let start = self.i;
                let mut text = String::new();
                let mut j = self.i;
                loop {
                    let line_start = j;
                    while self.at(j).is_some_and(|c| c != '\n') {
                        j += 1;
                    }
                    let line: String = self.cs[line_start..j].iter().map(|(_, c)| *c).collect();
                    let trailing = line.chars().rev().take_while(|c| *c == '\\').count();
                    if hd.joins && trailing % 2 == 1 && self.at(j) == Some('\n') {
                        text.push_str(&line[..line.len() - 1]);
                        j += 1;
                        continue;
                    }
                    text.push_str(&line);
                    break;
                }
                let cmp = if hd.strip_tabs {
                    text.trim_start_matches('\t')
                } else {
                    text.as_str()
                };
                let is_end = cmp == hd.delim;
                for k in start..j {
                    self.mark(
                        k,
                        if is_end {
                            Class::Operator
                        } else {
                            Class::Plain
                        },
                    );
                }
                self.i = j;
                if self.at(self.i) == Some('\n') {
                    self.i += 1;
                }
                if is_end {
                    break;
                }
            }
        }
    }

    /// The current code word is complete: colour it by what it turned out to be.
    fn end_word(&mut self) {
        let (w, top) = match self.stack.last_mut() {
            Some(t) if t.kind == Kind::Code => match t.word.take() {
                Some(w) => (w, t.top),
                None => return,
            },
            _ => return,
        };
        let pure = !w.nested;
        let text = w.prefix.clone();
        if pure && changes_the_reading(&text) {
            self.give_up(w.start);
            return;
        }
        let class = self.word_class(pure, &text);
        for i in &w.own {
            self.mark(*i, class);
        }
        if class == Class::Expansion
            && let Some(n) = assignment_name(&text)
        {
            // Only the name is the variable; what it is set to is plain.
            let mut b = 0;
            for i in &w.own {
                let len = self.cs[*i].1.len_utf8();
                if b >= n {
                    self.mark(*i, Class::Plain);
                }
                b += len;
            }
        }
        if pure && text == "[[" {
            self.push(Kind::Cond, w.start);
            self.top().boundary = true;
            // `push` marked the parent mid-word; `[[` was a word of its own.
            let n = self.stack.len();
            self.stack[n - 2].word = None;
            self.stack[n - 2].boundary = true;
        }
        if pure && text == "case" && !top {
            // A case pattern's `)` breaks the counting of `$(…)`, and bash
            // versions scan it differently.
            self.give_up(w.start);
        }
    }

    /// What a finished word is, and what it does to the context.
    fn word_class(&mut self, pure: bool, text: &str) -> Class {
        let t = self.stack.last_mut().expect("code context");
        // A case, waiting for its subject or `in`, or reading patterns.
        match t.case.last().copied() {
            Some(CaseAt::Subject) => {
                *t.case.last_mut().expect("checked") = CaseAt::WantIn;
                return Class::Plain;
            }
            Some(CaseAt::WantIn) if pure && text == "in" => {
                *t.case.last_mut().expect("checked") = CaseAt::Pattern;
                t.cmd_pos = false;
                return Class::Keyword;
            }
            Some(CaseAt::Pattern) => {
                if pure && text == "esac" {
                    t.case.pop();
                    t.cmd_pos = false;
                    return Class::Keyword;
                }
                return Class::Plain;
            }
            _ => {}
        }
        match t.expect {
            Expect::Target => {
                t.expect = Expect::Nothing;
                return Class::Plain;
            }
            Expect::LoopName => {
                t.expect = Expect::LoopIn;
                return Class::Expansion;
            }
            Expect::LoopIn => {
                t.expect = Expect::Nothing;
                if pure && text == "in" {
                    return Class::Keyword;
                }
                return Class::Plain;
            }
            Expect::FuncName => {
                t.expect = Expect::Nothing;
                return Class::Command;
            }
            Expect::Nothing => {}
        }
        if !t.cmd_pos {
            return Class::Plain;
        }
        if pure {
            let k = text;
            let keyword = matches!(
                k,
                "if" | "then"
                    | "elif"
                    | "else"
                    | "fi"
                    | "while"
                    | "until"
                    | "do"
                    | "done"
                    | "for"
                    | "select"
                    | "case"
                    | "esac"
                    | "function"
                    | "time"
                    | "coproc"
                    | "!"
                    | "{"
                    | "}"
                    | "[["
            );
            if keyword {
                match k {
                    "fi" | "done" | "}" => t.cmd_pos = false,
                    "esac" => {
                        t.case.pop();
                        t.cmd_pos = false;
                    }
                    "for" | "select" => {
                        t.expect = Expect::LoopName;
                        t.cmd_pos = false;
                    }
                    "case" => {
                        t.case.push(CaseAt::Subject);
                        t.cmd_pos = false;
                    }
                    "function" => {
                        t.expect = Expect::FuncName;
                        t.cmd_pos = false;
                    }
                    "[[" => t.cmd_pos = false,
                    _ => t.cmd_pos = true,
                }
                return Class::Keyword;
            }
        }
        if assignment_name(text).is_some() {
            // An assignment before a command leaves room for the command.
            return Class::Expansion;
        }
        // The command word.
        t.cmd_pos = false;
        Class::Command
    }

    // ---- [[ … ]] ------------------------------------------------------------

    fn cond(&mut self) {
        if self.continuation(Class::Plain) {
            return;
        }
        if self.newline_with_heredocs_pending() {
            return;
        }
        let Some(c) = self.at(self.i) else { return };
        let at = self.byte(self.i);
        match c {
            '#' if self.top().boundary => self.give_up(at),
            ' ' | '\t' | '\n' => {
                self.take(Class::Plain);
                self.cond_word_end();
                self.top().boundary = true;
            }
            '(' => {
                self.take(Class::Plain);
                let g = self.push_group(')', at);
                g.boundary = true;
            }
            ')' => self.give_up(at),
            '<' if self.la(1) == Some('<') => self.give_up(at),
            '&' | '|' | '<' | '>' | '!' => {
                self.take(Class::Operator);
                let t = self.top();
                t.cond_word.push(c);
                t.boundary = true;
            }
            '\\' => {
                self.take(Class::Plain);
                self.take(Class::Plain);
                let t = self.top();
                t.cond_word.push('\\');
                t.boundary = false;
            }
            '\'' => {
                self.take(Class::Quoted);
                self.push(Kind::Single, at);
                self.top_cond_impure();
            }
            '"' => {
                self.take(Class::Quoted);
                self.push(Kind::Double, at).flag = true;
                self.top_cond_impure();
            }
            '`' => {
                self.take(Class::Expansion);
                self.push(Kind::Backtick, at);
                self.top_cond_impure();
            }
            '$' => {
                self.dollar(false, Class::Plain);
                if let Some(t) = self.stack.iter_mut().rev().find(|c| c.kind == Kind::Cond) {
                    t.cond_word.push('$');
                }
            }
            _ => {
                self.take(Class::Plain);
                let t = self.top();
                t.cond_word.push(c);
                t.boundary = false;
            }
        }
        // `]]` is only the end once it is a whole word.
        if self.stack.last().is_some_and(|t| t.kind == Kind::Cond)
            && self.top().cond_word == "]]"
            && self
                .la(0)
                .is_none_or(|c| matches!(c, ' ' | '\t' | '\n' | ';' | '&' | '|' | ')'))
        {
            let end = self.i;
            self.mark(end - 1, Class::Keyword);
            self.mark(end - 2, Class::Keyword);
            self.pop();
            let t = self.top();
            t.boundary = true;
            t.cmd_pos = false;
            t.word = None;
        }
    }

    fn top_cond_impure(&mut self) {
        let n = self.stack.len();
        if n >= 2 && self.stack[n - 2].kind == Kind::Cond {
            self.stack[n - 2].cond_word.push('"');
        }
    }

    fn cond_word_end(&mut self) {
        self.top().cond_word.clear();
    }

    // ---- $(( … )) and (( … )) ----------------------------------------------

    fn arith(&mut self) {
        let class = if self.top().flag {
            Class::Expansion
        } else {
            Class::Plain
        };
        if self.continuation(class) {
            return;
        }
        if self.newline_with_heredocs_pending() {
            return;
        }
        let Some(c) = self.at(self.i) else { return };
        let at = self.byte(self.i);
        match c {
            '#' if self.top().boundary => self.give_up(at),
            '(' => {
                self.take(class);
                let t = self.top();
                t.depth += 1;
                t.boundary = true;
            }
            ')' => {
                if self.top().depth > 0 {
                    self.take(class);
                    let t = self.top();
                    t.depth -= 1;
                    t.boundary = false;
                } else if self.la(1) == Some(')') {
                    let dollar = self.top().flag;
                    let close = if dollar {
                        Class::Expansion
                    } else {
                        Class::Keyword
                    };
                    self.take(close);
                    self.take(close);
                    self.pop();
                    let t = self.top();
                    if dollar {
                        t.boundary = false;
                    } else {
                        // `((1))#c` is a comment: the command is a token of
                        // its own.
                        t.boundary = true;
                        t.cmd_pos = false;
                        t.word = None;
                    }
                } else {
                    // bash reads `$( (…) … )` here; not modelled.
                    self.give_up(at);
                }
            }
            '\\' => {
                self.take(class);
                self.take(class);
                self.top().boundary = false;
            }
            '\'' => {
                self.take(Class::Quoted);
                self.push(Kind::Single, at);
            }
            '"' => {
                self.take(Class::Quoted);
                self.push(Kind::Double, at).flag = true;
            }
            '`' => {
                self.take(Class::Expansion);
                self.push(Kind::Backtick, at);
            }
            '$' => self.dollar(false, class),
            c if c.is_ascii_alphanumeric() || c == '_' || c == '.' => {
                self.take(class);
                self.top().boundary = false;
            }
            _ => {
                // Blanks and arithmetic operators separate tokens.
                self.take(class);
                self.top().boundary = true;
            }
        }
    }

    // ---- words read whole ---------------------------------------------------

    fn group(&mut self) {
        if self.continuation(Class::Plain) {
            return;
        }
        if self.newline_with_heredocs_pending() {
            return;
        }
        let Some(c) = self.at(self.i) else { return };
        let at = self.byte(self.i);
        let close = self.top().close;
        match c {
            '#' if self.top().boundary => self.give_up(at),
            ' ' | '\t' | '\n' => {
                self.take(Class::Plain);
                self.top().boundary = true;
            }
            '(' if close == ')' => {
                self.take(Class::Plain);
                let t = self.top();
                t.depth += 1;
                t.boundary = true;
            }
            ')' | ']' if c == close => {
                self.take(Class::Plain);
                if self.top().depth > 0 {
                    let t = self.top();
                    t.depth -= 1;
                    t.boundary = false;
                } else {
                    self.pop();
                    self.top().boundary = false;
                }
            }
            ')' => self.give_up(at),
            '<' if self.la(1) == Some('<') => self.give_up(at),
            '|' | '&' | ';' | '<' | '>' => {
                self.take(Class::Plain);
                self.top().boundary = true;
            }
            '\\' => {
                self.take(Class::Plain);
                self.take(Class::Plain);
                self.top().boundary = false;
            }
            '\'' => {
                self.take(Class::Quoted);
                self.push(Kind::Single, at);
            }
            '"' => {
                self.take(Class::Quoted);
                self.push(Kind::Double, at).flag = true;
            }
            '`' => {
                self.take(Class::Expansion);
                self.push(Kind::Backtick, at);
            }
            '$' => self.dollar(false, Class::Plain),
            _ => {
                self.take(Class::Plain);
                self.top().boundary = false;
            }
        }
    }

    // ---- quotes -------------------------------------------------------------

    fn single(&mut self) {
        if self.newline_with_heredocs_pending() {
            return;
        }
        // No escapes and no continuations in single quotes: `'` ends it.
        let c = self.at(self.i);
        self.mark(self.i, Class::Quoted);
        self.i += 1;
        if c == Some('\'') {
            self.pop();
        }
    }

    fn ansi(&mut self) {
        if self.newline_with_heredocs_pending() {
            return;
        }
        let c = self.at(self.i);
        self.mark(self.i, Class::Quoted);
        self.i += 1;
        match c {
            Some('\\') => {
                if self.at(self.i).is_some() {
                    self.mark(self.i, Class::Quoted);
                    self.i += 1;
                }
            }
            Some('\'') => self.pop(),
            _ => {}
        }
    }

    fn double(&mut self) {
        if self.continuation(Class::Quoted) {
            return;
        }
        if self.newline_with_heredocs_pending() {
            return;
        }
        let Some(c) = self.at(self.i) else { return };
        let at = self.byte(self.i);
        match c {
            '\\' => {
                self.take(Class::Quoted);
                if self.at(self.i).is_some() {
                    self.mark(self.i, Class::Quoted);
                    self.i += 1;
                }
            }
            '"' => {
                self.take(Class::Quoted);
                self.pop();
            }
            '`' => {
                self.take(Class::Expansion);
                self.push(Kind::Backtick, at);
            }
            '$' => self.dollar(true, Class::Quoted),
            _ => {
                self.take(Class::Quoted);
            }
        }
    }

    fn brace(&mut self) {
        if self.continuation(Class::Expansion) {
            return;
        }
        if self.newline_with_heredocs_pending() {
            return;
        }
        let Some(c) = self.at(self.i) else { return };
        let at = self.byte(self.i);
        let in_double = self.top().flag;
        match c {
            '}' => {
                self.take(Class::Expansion);
                self.pop();
                self.top().boundary = false;
            }
            '\\' => {
                self.take(Class::Expansion);
                self.take(Class::Expansion);
            }
            // Inside double quotes, whether `'` quotes here depends on posix mode.
            '\'' if in_double => self.give_up(at),
            '\'' => {
                self.take(Class::Quoted);
                self.push(Kind::Single, at);
            }
            '"' => {
                self.take(Class::Quoted);
                self.push(Kind::Double, at).flag = true;
            }
            '`' => {
                self.take(Class::Expansion);
                self.push(Kind::Backtick, at);
            }
            '$' => self.dollar(in_double, Class::Expansion),
            _ => {
                self.take(Class::Expansion);
            }
        }
    }

    fn backtick(&mut self) {
        if self.continuation(Class::Expansion) {
            return;
        }
        if self.newline_with_heredocs_pending() {
            return;
        }
        let Some(c) = self.at(self.i) else { return };
        let at = self.byte(self.i);
        match c {
            '\\' => {
                self.take(Class::Expansion);
                self.take(Class::Expansion);
            }
            '`' => {
                self.take(Class::Expansion);
                self.pop();
                self.top().boundary = false;
            }
            '#' => {
                // A backtick's inside is read later, as its own script; a `#`
                // that could start a comment there is not ours to judge.
                let prev = if self.i == 0 {
                    None
                } else {
                    self.at(self.i - 1)
                };
                if matches!(prev, Some(' ' | '\t' | '\n' | '`' | ';' | '&' | '|' | '(')) {
                    self.give_up(at);
                } else {
                    self.take(Class::Expansion);
                }
            }
            _ => {
                self.take(Class::Expansion);
            }
        }
    }
}

#[cfg(test)]
mod tests;
