use super::*;

/// One letter per character of `script`, for reading a case as the script with
/// what the lexer made of it underneath: `.` plain, `C` command, `K` keyword,
/// `O` operator, `Q` quoted, `E` expansion, `-` comment. A newline is kept.
fn shadow(script: &str) -> String {
    let classes = classify(script);
    script
        .char_indices()
        .map(|(b, c)| {
            if c == '\n' {
                return '\n';
            }
            match classes[b] {
                Class::Plain => '.',
                Class::Command => 'C',
                Class::Keyword => 'K',
                Class::Operator => 'O',
                Class::Quoted => 'Q',
                Class::Expansion => 'E',
                Class::Comment => '-',
            }
        })
        .collect()
}

/// The classes of the first occurrence of `needle` in `script`.
fn classes_of(script: &str, needle: &str) -> Vec<Class> {
    let at = script
        .find(needle)
        .unwrap_or_else(|| panic!("{needle:?} not in {script:?}"));
    classify(script)[at..at + needle.len()].to_vec()
}

fn assert_comment(script: &str, needle: &str) {
    let got = classes_of(script, needle);
    assert!(
        got.iter().all(|c| *c == Class::Comment),
        "{needle:?} should be a comment in {script:?}:\n{script}\n{}",
        shadow(script)
    );
}

/// Nothing of `live` — code bash runs — is drawn as a comment or a string.
fn assert_live(script: &str, live: &str) {
    let got = classes_of(script, live);
    assert!(
        !got.iter()
            .any(|c| matches!(c, Class::Comment | Class::Quoted)),
        "{live:?} runs, but is drawn inert in {script:?}:\n{script}\n{}",
        shadow(script)
    );
}

/// Comments are recognised where bash starts a token — after whitespace, at a
/// line's start, and right after an operator, as bash 5.2 does (`a;#b` is
/// `a` and a comment).
#[test]
fn comments_start_where_bash_starts_a_token() {
    assert_comment("# x", "# x");
    assert_comment("a #b", "#b");
    assert_comment("a\t#b", "#b");
    assert_comment("    # indented", "# indented");
    assert_comment("a;#b", "#b");
    assert_comment("a|# b\ncat", "# b");
    assert_comment("a&#b", "#b");
    assert_comment("(a)#b", "#b");
    assert_comment("((1))#b", "#b");
    assert_comment("a \\\n#b", "#b");
    assert_comment("case x in a) b;;# c\nesac", "# c");
}

/// A comment runs to its newline and no further — a `\` at its end continues
/// nothing — so the next line is code again.
#[test]
fn a_comment_ends_at_its_newline_even_after_a_backslash() {
    let s = "# c \\\nrm x";
    assert_comment(s, "# c \\");
    assert_eq!(
        classes_of(s, "rm"),
        vec![Class::Command; 2],
        "{}",
        shadow(s)
    );
}

/// The case that decided I1: a comment the lexer failed to see would have its
/// `'` open a quote, and the live line after it would be drawn as a string.
#[test]
fn a_quote_inside_a_comment_opens_nothing() {
    let s = "systemctl restart nginx;# don't wait\nrm -rf /var/www/*\necho 'done'";
    assert_comment(s, "# don't wait");
    assert_live(s, "rm -rf /var/www/*");
    assert_eq!(
        classes_of(s, "rm"),
        vec![Class::Command; 2],
        "{}",
        shadow(s)
    );
}

/// I1, case by case: every `LIVE` here is code bash runs (each checked against
/// bash 5.2), and none of it may be drawn as a comment or a string.
#[test]
fn nothing_bash_runs_is_drawn_as_a_comment_or_a_string() {
    let cases = [
        // A `#` inside a word, however it got there.
        "echo a#b; echo LIVE",
        "echo a\\ #b; echo LIVE",
        "echo \\#b; echo LIVE",
        "echo \\;#b; echo LIVE",
        "echo safe\\\n#; echo LIVE",
        "echo '#'; echo LIVE",
        "echo \"#\"; echo LIVE",
        "echo $'#'; echo LIVE",
        "echo \"a\"#b; echo LIVE",
        "echo $(true)#b; echo LIVE",
        "x=abc; echo ${#x}; echo LIVE",
        "set -- 1 2; echo $#; echo LIVE",
        "x=a#b; echo ${x#a}; echo LIVE",
        "echo $(( 16#ff )); echo LIVE",
        "(( 2#101 )); echo LIVE",
        "[[ $x == *#* ]] && echo LIVE",
        "case x in \\#*) echo LIVE;; esac",
        // Only a space or a tab ends a word: NBSP, CR, VT and FF do not.
        "echo a\u{a0}#b; echo LIVE",
        "echo a\r#b; echo LIVE",
        "echo a\u{b}#b; echo LIVE",
        "echo a\u{c}#b; echo LIVE",
        // An escape sequence the screen hides is still part of the word —
        // and so is a bare ESC, which bash reads as a word character.
        "echo x \u{1b}[0m# y; echo LIVE",
        "echo x \u{1b}#y; echo LIVE",
        // Words bash reads whole, where a `#` after a space is still inside.
        "[[ $x =~ ^(a| #b)$ ]] && echo LIVE",
        "arr[ #]=x; echo LIVE",
        "[[ $x == @( #a|b) ]] && echo LIVE",
        // Comments inside substitutions: bash has them, this lexer gives up.
        "x=$(true # it's )\necho LIVE #'\n)",
        "x=$(true;# c\n)\necho LIVE",
        "x=`true # c`\necho LIVE",
    ];
    for s in cases {
        assert_live(s, "LIVE");
    }
}

/// Scripts of the kinds a plan carries, and the awkward cases above, for the
/// oracle and the fuzz to chew on.
fn corpus() -> Vec<String> {
    let mut v: Vec<String> = [
        "set -euo pipefail\n# update the cache\napt-get update -y # quietly\n",
        "for h in a b c; do\n  echo \"$h\" # each\ndone\n",
        "if [[ -f /etc/os-release ]]; then\n  . /etc/os-release\nfi # done\n",
        "case \"$1\" in\n  start) systemctl start x ;; # go\n  # a pattern comment\n  stop|halt) systemctl stop x ;;\nesac\n",
        "cat > /etc/motd <<EOF\n# not a comment: text\nhello $USER\nEOF\necho after # c\n",
        "cat <<'EOF' | sudo tee /x\n'unbalanced\n# text\nEOF\n# real\n",
        "x=$(date +%s) # stamp\necho \"${x:-none}\" # print\n",
        "f() { echo in; } # define\nf # call\n",
        "while read -r l; do echo \"$l\"; done < /etc/hosts # read\n",
        "echo a;#b\necho c|# d\ncat\n",
        "arr=(one two three) # list\necho ${#arr[@]}\n",
        "(( n = 1 << 3 )) # shift\necho $(( n#1 ))\n",
        "echo \"multi\nline # inside\n\" # outside\n",
        "echo 'single\n# inside\n' # outside\n",
        "echo done # done\n",
        "for ((i=0; i<3; i++)); do echo $i; done # loop\n",
        "sudo systemctl restart nginx;# don't wait\nrm -f /tmp/x\n",
        "echo safe\\\n#; echo LIVE\n",
        "x=abc; echo ${#x} ${x#a} $# # three\n",
        "[[ $x == *#* ]] && echo yes # glob\n",
        "echo `date` # tick\n",
        "echo <(ls) # proc\n",
        "cat <<A <<'B'\nA body # a\nA\nB body # b\nB\necho end # e\n",
        "cat <<-EOF\n\tindented # body\n\tEOF\necho after # c\n",
        "echo x \\\n  -y # cont\n",
        "{ echo grouped; } # brace\n",
        "! false # not\n",
        "time sleep 0 # t\n",
        "echo $'esc \\' q' # ansi\n",
        "echo ${x:-'dflt'} # d\n",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    for s in [
        "echo a#b; echo LIVE",
        "echo \\#b; echo LIVE",
        "echo '#'; echo LIVE",
        "x=a#b; echo ${x#a}; echo LIVE",
        "(( 2#101 )); echo LIVE",
    ] {
        v.push(format!("{s}\n"));
    }
    v
}

/// The script with the bytes this lexer calls comments taken out.
fn without_comments(script: &str) -> String {
    let classes = classify(script);
    script
        .char_indices()
        .filter(|(b, _)| classes[*b] != Class::Comment)
        .map(|(_, c)| c)
        .collect()
}

/// bash's own reading of `script`, comments already gone: the body of a
/// function, printed back. `None` if bash will not parse it.
fn bash_reads(script: &str) -> Option<String> {
    let wrapped = format!("f() {{\n{script}\n}}\ndeclare -f f");
    let out = std::process::Command::new("bash")
        .args(["--norc", "--noprofile", "-c", &wrapped])
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// I1 against bash itself. `declare -f` prints a function as bash parsed it,
/// without its comments; so taking out what this lexer calls comments must
/// leave bash's reading unchanged. Had it called live code a comment, taking
/// that out would change what bash reads. Parse-only: nothing here runs.
#[test]
fn comments_are_exactly_what_bash_discards() {
    if std::process::Command::new("bash")
        .arg("--version")
        .output()
        .is_err()
    {
        eprintln!("no bash on PATH; the oracle is skipped");
        return;
    }
    let mut checked = 0;
    for s in corpus() {
        let Some(bash) = bash_reads(&s) else {
            continue;
        };
        let ours = bash_reads(&without_comments(&s));
        assert_eq!(
            ours.as_deref(),
            Some(bash.as_str()),
            "taking out our comments changed what bash reads:\n{s}\n{}",
            shadow(&s)
        );
        checked += 1;
    }
    assert!(checked >= 30, "only {checked} corpus scripts parsed");
}

/// I3: an unterminated quote is a script bash will not run, and this lexer
/// vouches for nothing after it — least of all that the next lines are
/// comments or strings.
#[test]
fn an_unterminated_quote_turns_the_rest_plain() {
    let s = "echo ok\necho 'oops\n# no\nrm -rf /srv";
    let at = s.find('\'').unwrap();
    let c = classify(s);
    assert!(c[at..].iter().all(|c| *c == Class::Plain), "{}", shadow(s));
    assert_eq!(
        classes_of(s, "echo"),
        vec![Class::Command; 4],
        "before it, still coloured"
    );
}

#[test]
fn an_unterminated_substitution_turns_the_rest_plain() {
    let s = "x=$(date\n# no\nrm -rf /srv";
    let at = s.find('$').unwrap();
    assert!(
        classify(s)[at..].iter().all(|c| *c == Class::Plain),
        "{}",
        shadow(s)
    );
}

/// bash 5.2 drops a NUL before reading on; what it makes of the rest is not
/// guessed at.
#[test]
fn a_nul_ends_what_the_lexer_vouches_for() {
    let s = "echo x \0#y; echo LIVE";
    let at = s.find('\0').unwrap();
    assert!(
        classify(s)[at..].iter().all(|c| *c == Class::Plain),
        "{}",
        shadow(s)
    );
}

/// An alias can make a later line read as anything; the lexer stops there.
#[test]
fn aliases_and_history_expansion_stop_the_lexer() {
    for s in [
        "alias q=\"'\"\necho LIVE",
        "shopt -s expand_aliases\necho LIVE",
        "set -o histexpand\necho LIVE",
    ] {
        assert!(
            classes_of(s, "LIVE").iter().all(|c| *c == Class::Plain),
            "{}",
            shadow(s)
        );
    }
}

/// A heredoc's body is text, not code: its `#` is no comment and its `'` opens
/// nothing — and code comes back after the delimiter line.
#[test]
fn a_heredoc_body_is_plain_and_code_resumes_after_it() {
    let s = "cat > /etc/x <<EOF\n# text 'unbalanced\nEOF\nrm -f /tmp/y # c";
    assert!(
        classes_of(s, "# text 'unbalanced")
            .iter()
            .all(|c| *c == Class::Plain),
        "{}",
        shadow(s)
    );
    assert_eq!(
        classes_of(s, "EOF\nrm")[..3],
        [Class::Operator; 3],
        "the delimiter line"
    );
    assert_eq!(
        classes_of(s, "rm"),
        vec![Class::Command; 2],
        "{}",
        shadow(s)
    );
    assert_comment(s, "# c");
}

#[test]
fn quoted_and_dashed_delimiters_are_recognised() {
    for s in [
        "cat <<'E'\n$x # t\nE\nrm z",
        "cat <<\"E\"\n$x # t\nE\nrm z",
        "cat <<\\E\n$x # t\nE\nrm z",
        "cat <<-E\n\t$x # t\n\tE\nrm z",
    ] {
        assert!(
            classes_of(s, "# t").iter().all(|c| *c == Class::Plain),
            "{s:?}\n{}",
            shadow(s)
        );
        assert_eq!(
            classes_of(s, "rm"),
            vec![Class::Command; 2],
            "{s:?}\n{}",
            shadow(s)
        );
    }
}

/// Two heredocs on one line read their bodies in order, after the rest of
/// that line has been read as code.
#[test]
fn two_heredocs_on_a_line_read_their_bodies_in_order() {
    let s = "cat <<A <<'B' | grep x # c\nbody a\nA\nbody b\nB\nrm z";
    assert_comment(s, "# c");
    assert_eq!(
        classes_of(s, "grep"),
        vec![Class::Command; 4],
        "{}",
        shadow(s)
    );
    for text in ["body a", "body b"] {
        assert!(
            classes_of(s, text).iter().all(|c| *c == Class::Plain),
            "{}",
            shadow(s)
        );
    }
    assert_eq!(
        classes_of(s, "rm"),
        vec![Class::Command; 2],
        "{}",
        shadow(s)
    );
}

/// `<<<` is a here-string and `<<` inside arithmetic a shift: neither starts a
/// body, so the next line is code.
#[test]
fn a_here_string_and_a_shift_are_not_heredocs() {
    for s in [
        "cat <<< hi\nrm z",
        "echo $((1<<2))\nrm z",
        "(( x = 1 << 2 ))\nrm z",
    ] {
        assert_eq!(
            classes_of(s, "rm"),
            vec![Class::Command; 2],
            "{s:?}\n{}",
            shadow(s)
        );
    }
}

/// An unquoted body line ending in an odd run of `\` joins the next, so a
/// delimiter there is not one; trailing blanks keep a line from matching.
#[test]
fn a_body_ends_only_at_its_exact_delimiter_line() {
    let s = "cat <<E\nx \\\nE\nstill body\nE \nE\nrm z";
    assert!(
        classes_of(s, "still body")
            .iter()
            .all(|c| *c == Class::Plain),
        "{}",
        shadow(s)
    );
    assert_eq!(
        classes_of(s, "rm"),
        vec![Class::Command; 2],
        "{}",
        shadow(s)
    );
}

/// A continuation inside the operator still makes a heredoc.
#[test]
fn a_continuation_inside_the_operator_still_makes_a_heredoc() {
    let s = "cat <\\\n<E\n# text\nE\nrm z";
    assert!(
        classes_of(s, "# text").iter().all(|c| *c == Class::Plain),
        "{}",
        shadow(s)
    );
    assert_eq!(
        classes_of(s, "rm"),
        vec![Class::Command; 2],
        "{}",
        shadow(s)
    );
}

/// A delimiter this lexer cannot read exactly, or a heredoc inside `$(…)`,
/// where bash versions differ: the rest is plain.
#[test]
fn a_heredoc_it_cannot_read_turns_the_rest_plain() {
    for s in [
        "cat <<$x\n# t\n$x\nrm z",
        "cat <<E\"OF\"\n# t\nEOF\nrm z",
        "x=$(cat <<E\n# t\nE\n)\nrm z",
    ] {
        assert!(
            classes_of(s, "rm").iter().all(|c| *c == Class::Plain),
            "{s:?}\n{}",
            shadow(s)
        );
        assert!(
            !classify(s).contains(&Class::Comment),
            "{s:?}\n{}",
            shadow(s)
        );
    }
}

/// With no terminator bash runs the heredoc to the end, warning: all body.
#[test]
fn a_heredoc_without_a_terminator_is_body_to_the_end() {
    let s = "cat <<E\n# t\nrm z";
    assert!(
        classes_of(s, "rm z").iter().all(|c| *c == Class::Plain),
        "{}",
        shadow(s)
    );
}

/// A double-quoted string spans lines, its `#` inside it; the `#` after it is
/// a comment.
#[test]
fn a_double_quoted_string_spans_lines() {
    let s = "echo \"a\n# inside\n\" # real";
    assert!(
        classes_of(s, "# inside")
            .iter()
            .all(|c| *c == Class::Quoted),
        "{}",
        shadow(s)
    );
    assert_comment(s, "# real");
}

#[test]
fn an_ansi_string_keeps_escaped_quotes() {
    let s = "echo $'a \\' b' # c";
    assert!(
        classes_of(s, "b'").iter().all(|c| *c == Class::Quoted),
        "{}",
        shadow(s)
    );
    assert_comment(s, "# c");
}

/// A substitution inside a string, with a string inside it, closes where bash
/// closes it.
#[test]
fn nested_substitutions_close_where_bash_closes_them() {
    let s = "echo \"$(echo \")\")\" # c";
    assert_comment(s, "# c");
}

/// The first word of a command is what runs; the same word as an argument is
/// not a keyword.
#[test]
fn the_first_word_is_the_command_and_arguments_are_not() {
    let s = "echo done";
    assert_eq!(shadow(s), "CCCC.....");
    let s = "if true; then echo done; fi";
    assert_eq!(shadow(s), "KK.CCCCO.KKKK.CCCC.....O.KK");
}

/// Every list operator starts a new command.
#[test]
fn every_list_operator_starts_a_new_command() {
    let s = "a; b && c || d | e & f |& g";
    for w in ["a", "b", "c", "d", "e", "f", "g"] {
        assert_eq!(classes_of(s, w), vec![Class::Command], "{w}: {}", shadow(s));
    }
}

/// An assignment before a command leaves room for it, and names the variable;
/// a redirection before one does not take its place.
#[test]
fn assignments_and_redirections_leave_the_command_position_open() {
    let s = "LANG=C sort x";
    assert_eq!(shadow(s), "EEEE...CCCC..");
    let s = ">out echo hi";
    assert_eq!(
        classes_of(s, "echo"),
        vec![Class::Command; 4],
        "{}",
        shadow(s)
    );
    let s = "2>/dev/null ls";
    assert_eq!(
        classes_of(s, "2>"),
        vec![Class::Operator; 2],
        "{}",
        shadow(s)
    );
    assert_eq!(
        classes_of(s, "ls"),
        vec![Class::Command; 2],
        "{}",
        shadow(s)
    );
}

/// A continued line continues the command: its first word is an argument.
#[test]
fn a_continued_line_continues_the_command() {
    let s = "apt-get install \\\n  -y nginx";
    assert_eq!(classes_of(s, "-y"), vec![Class::Plain; 2], "{}", shadow(s));
}

/// Bytes the fuzz makes of these, any order: every byte gets a class, and the
/// lexer neither panics nor loops.
#[test]
fn every_byte_gets_a_class_and_nothing_panics() {
    let tokens = [
        "#", "'", "\"", "\\", "\n", ";", "(", ")", "$(", "${", "}", "`", "<<E", "E", "[[", "]]",
        " ", "\t", "a", "$", "((", "))", "<", ">", "|", "&", "=", "é", "\u{1b}", "\0", "case",
        "in", "esac",
    ];
    let mut seed: u64 = 0x9e37_79b9_7f4a_7c15;
    for _ in 0..2000 {
        let mut s = String::new();
        for _ in 0..(seed % 24) {
            seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            s.push_str(tokens[(seed >> 33) as usize % tokens.len()]);
        }
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        assert_eq!(classify(&s).len(), s.len(), "{s:?}");
    }
}

/// `script_rows` has the lines `lines` has — no row for an empty script, none
/// after a final newline — and shows a `\r` rather than dropping it.
#[test]
fn script_rows_has_the_lines_that_lines_has() {
    let text = |s: &str| -> Vec<String> {
        script_rows(s, 80)
            .iter()
            .map(|r| r.spans.iter().map(|s| s.content.as_ref()).collect())
            .collect()
    };
    assert!(text("").is_empty());
    assert_eq!(text("a"), ["a"]);
    assert_eq!(text("a\n"), ["a"]);
    assert_eq!(text("a\n\nb"), ["a", "", "b"]);
    assert_eq!(text("a\r\nb"), ["a^M", "b"]);
}

/// The words that are commands in `script`, in order.
fn commands(script: &str) -> Vec<String> {
    let classes = classify(script);
    let mut out = Vec::new();
    let mut cur = String::new();
    for (b, c) in script.char_indices() {
        if classes[b] == Class::Command {
            cur.push(c);
        } else if !cur.is_empty() {
            out.push(std::mem::take(&mut cur));
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// `sudo` runs its first word that is not an option or an option's value: in
/// `sudo -u postgres psql` that is `psql`, not `postgres`.
#[test]
fn sudo_runs_the_command_after_its_options() {
    assert_eq!(
        commands("sudo systemctl restart nginx"),
        ["sudo", "systemctl"]
    );
    assert_eq!(
        commands("sudo -u postgres psql -c 'select 1'"),
        ["sudo", "psql"]
    );
    assert_eq!(
        commands("sudo -uroot id"),
        ["sudo", "id"],
        "a value attached"
    );
    assert_eq!(
        commands("sudo -Eu root env"),
        ["sudo", "env"],
        "a cluster ending in -u"
    );
    assert_eq!(
        commands("sudo -uEroot id"),
        ["sudo", "id"],
        "-u's value is the rest"
    );
    assert_eq!(commands("sudo --user root id"), ["sudo", "id"]);
    assert_eq!(commands("sudo --user=root id"), ["sudo", "id"]);
    assert_eq!(commands("sudo -n -H -- ls /root"), ["sudo", "ls"]);
    let s = "sudo DEBIAN_FRONTEND=noninteractive apt-get install -y nginx";
    assert_eq!(commands(s), ["sudo", "apt-get"]);
    assert_eq!(
        classes_of(s, "DEBIAN_FRONTEND"),
        vec![Class::Expansion; 15],
        "{}",
        shadow(s)
    );
}

/// `sudo -e` edits the files it names; nothing after it runs as a command.
#[test]
fn sudo_edit_names_files_not_a_command() {
    assert_eq!(commands("sudo -e /etc/hosts"), ["sudo"]);
}

/// The other wrappers, by the same rule: each option table decides which word
/// is the value and which the command; `timeout` takes a duration first.
#[test]
fn other_wrappers_run_the_command_after_their_options() {
    assert_eq!(commands("env -i PATH=/bin sh -c 'x'"), ["env", "sh"]);
    assert_eq!(commands("env -u HOME -C /tmp ls"), ["env", "ls"]);
    assert_eq!(commands("nohup ./run.sh &"), ["nohup", "./run.sh"]);
    assert_eq!(commands("nice -n 10 make -j4"), ["nice", "make"]);
    assert_eq!(commands("nice -10 make"), ["nice", "make"]);
    assert_eq!(commands("timeout 5s curl x"), ["timeout", "curl"]);
    assert_eq!(
        commands("timeout -s KILL 10 sleep 60"),
        ["timeout", "sleep"]
    );
    assert_eq!(commands("exec -a name bash"), ["exec", "bash"]);
    assert_eq!(commands("command -v git"), ["command", "git"]);
    assert_eq!(commands("xargs -n 1 -I {} rm {}"), ["xargs", "rm"]);
    assert_eq!(commands("doas -u www touch f"), ["doas", "touch"]);
}

/// A wrapper's command may be another wrapper, and so on down.
#[test]
fn wrappers_nest() {
    assert_eq!(
        commands("sudo env FOO=1 nice -n 5 timeout 30 ./job --fast"),
        ["sudo", "env", "nice", "timeout", "./job"]
    );
}

/// A redirection inside a wrapped command does not lose the wrapper's place,
/// and a list operator or newline ends it.
#[test]
fn a_wrapper_keeps_its_place_across_redirections_and_ends_with_the_command() {
    assert_eq!(commands("sudo -u root >/tmp/log ls"), ["sudo", "ls"]);
    assert_eq!(commands("sudo 2>/dev/null ls"), ["sudo", "ls"]);
    assert_eq!(commands("sudo -u && ls"), ["sudo", "ls"], "a new command");
    assert_eq!(commands("sudo\nls"), ["sudo", "ls"]);
    assert_eq!(commands("sudo ls | grep x"), ["sudo", "ls", "grep"]);
}

/// Only in command position: `sudo` as an argument wraps nothing.
#[test]
fn a_wrapper_named_as_an_argument_wraps_nothing() {
    assert_eq!(commands("echo sudo ls"), ["echo"]);
    assert_eq!(commands("which sudo env"), ["which"]);
}
