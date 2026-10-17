//! The `type` example used by adversarial testers: options, the macro double, the output
//! format, and exit codes. The example's code is compiled into this test as a module.

#[allow(dead_code)]
#[path = "../examples/type.rs"]
mod type_example;

/// Runs the example with `args`; returns (exit code, stdout, stderr).
fn cli(args: &[&str]) -> (u8, String, String) {
    let args: Vec<String> = args.iter().map(|arg| (*arg).to_owned()).collect();
    let mut out = Vec::new();
    let mut err = Vec::new();
    let code = type_example::run(&args, &mut out, &mut err);
    let out = String::from_utf8(out).expect("stdout is UTF-8");
    let err = String::from_utf8(err).expect("stderr is UTF-8");
    (code, out, err)
}

/// The quoted text of the `final:` line.
fn final_text(args: &[&str]) -> String {
    let (code, out, err) = cli(args);
    assert_eq!(code, 0, "{args:?}: {err}");
    let line = out
        .lines()
        .last()
        .and_then(|line| line.strip_prefix("final: \""))
        .and_then(|line| line.strip_suffix('"'))
        .unwrap_or_else(|| panic!("final line in {out:?}"));
    line.to_owned()
}

#[test]
fn prints_every_step_and_the_final_text_in_utf8() {
    let (code, out, err) = cli(&["aa"]);
    assert_eq!(code, 0);
    assert!(err.is_empty());
    assert_eq!(
        out,
        "step 1 a: Tx(0,\"\",false,None,false)\n\
         step 2 a: Tx(1,\"â\",true,None,false)\n\
         final: \"â\"\n"
    );
    let (_, out, _) = cli(&["ko{SPACE}{CTRL+SPACE}{ENTER}"]);
    assert_eq!(
        out,
        "step 1 k: Tx(0,\"\",false,None,false)\n\
         step 2 o: Tx(0,\"\",false,None,false)\n\
         step 3 {SPACE}: Tx(0,\"\",false,None,true)\n\
         step 4 {CTRL+SPACE}: Tx(0,\"\",false,None,true)\n\
         step 5 {ENTER}: Tx(0,\"\",false,None,false)\n\
         final: \"ko  \\r\"\n"
    );
    // Typed whitespace characters are labelled by code point.
    let (_, out, _) = cli(&["a b"]);
    assert!(out.contains("step 2 U+0020: "), "{out}");
}

#[test]
fn options_select_method_style_normalization_and_switches() {
    assert_eq!(final_text(&["tieengs"]), "tiếng");
    assert_eq!(final_text(&["--method", "vni", "tie6ng1"]), "tiếng");
    assert_eq!(final_text(&["--method", "VIQR", "tie^'ng"]), "tiếng");
    assert_eq!(final_text(&["hoaf"]), "hòa");
    assert_eq!(final_text(&["--modern", "hoaf"]), "hoà");
    assert_eq!(final_text(&["--nfd", "hoaf"]), "ho\u{300}a");
    assert_eq!(final_text(&["--preserve", "hoaf"]), "hòa");
    assert_eq!(final_text(&["kw"]), "kw");
    assert_eq!(final_text(&["--no-spell", "kw"]), "kư");
    assert_eq!(final_text(&["after{SPACE}"]), "after ");
    assert_eq!(final_text(&["--no-restore", "after{SPACE}"]), "àter ");
    assert_eq!(final_text(&["--no-free", "did"]), "did");
    assert_eq!(final_text(&["--off", "aa"]), "aa");
    assert_eq!(final_text(&["--context", "8", "nghieengsss"]), "nghiêngsss");
}

#[test]
fn macro_double_matches_exactly_then_adapts_case() {
    let args = ["--macro", "ko=không", "vn=Việt Nam"];
    let with = |script: &str| {
        let mut all = args.to_vec();
        all.push(script);
        final_text(&all)
    };
    assert_eq!(with("ko{SPACE}"), "không ");
    assert_eq!(with("Ko{SPACE}"), "Không ");
    assert_eq!(with("KO{SPACE}"), "KHÔNG ");
    assert_eq!(with("VN."), "VIỆT NAM.");
    assert_eq!(
        final_text(&["--no-autocase", "--macro", "ko=không", "KO{SPACE}"]),
        "KO "
    );
    assert_eq!(
        final_text(&["--off", "--macro", "ko=không", "ko{SPACE}"]),
        "ko "
    );
    assert_eq!(
        final_text(&[
            "--off",
            "--macros-when-off",
            "--macro",
            "ko=không",
            "ko{SPACE}"
        ]),
        "không "
    );
    // An exact entry wins over a case-insensitive one.
    assert_eq!(
        final_text(&["--macro", "ko=không", "KO=Kosovo", "KO{SPACE}"]),
        "Kosovo "
    );
}

/// The example has no effect beyond its own output: it starts no other program (such as one
/// that switches the console code page) and leaves the console settings alone. Standard
/// output already reaches a console through its Unicode API and a pipe as UTF-8. Checked on
/// the source: a runtime check needs a real console, and a regression would then change the
/// console of whoever runs the tests.
#[test]
fn example_leaves_the_console_code_page_alone() {
    const SOURCE: &str = include_str!("../examples/type.rs");
    for forbidden in ["Command::new", "chcp", "SetConsoleCP", "SetConsoleOutputCP"] {
        assert!(
            !SOURCE.contains(forbidden),
            "examples/type.rs uses {forbidden}"
        );
    }
}

#[test]
fn usage_and_script_errors_exit_with_two() {
    for args in [
        &[][..],
        &["--method"][..],
        &["--method", "dvorak", "a"][..],
        &["--context", "lots", "a"][..],
        &["--macro", "a"][..],
        &["--bogus", "a"][..],
    ] {
        let (code, out, err) = cli(args);
        assert_eq!(code, 2, "{args:?}");
        assert!(out.is_empty(), "{args:?}");
        assert!(err.starts_with("error: "), "{args:?}: {err}");
        assert!(err.contains("usage: type"), "{args:?}: {err}");
    }
    let (code, out, err) = cli(&["--help"]);
    assert_eq!(code, 0);
    assert!(out.starts_with("usage: type"), "{out}");
    assert!(err.is_empty());
    let (code, out, err) = cli(&["ab{NOPE}"]);
    assert_eq!(code, 2);
    assert!(out.is_empty());
    assert!(err.contains("invalid script"), "{err}");
}
