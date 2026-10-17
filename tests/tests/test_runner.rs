//! The `raumakey-test-runner` command line, report, and exit codes (plan §12, design §15.3).
//!
//! The parsing and report tests call `raumakey_tests::runner` directly; the process tests run
//! the built binary (`CARGO_BIN_EXE_raumakey-test-runner`).

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command as Process, Output};

use raumakey_tests::runner::{self, Command, Environment, Report, UsageError, parse_args};
use raumakey_tests::vectors::{self, OsInfo, Status};
use serde_json::Value;

fn parse(args: &[&str]) -> Result<Command, UsageError> {
    parse_args(args.iter().map(OsString::from))
}

/// An environment that no real host reports, to tell the runner's values from the probe's.
fn fake_environment() -> Environment {
    Environment {
        os: OsInfo {
            name: "windows".to_owned(),
            build: Some("26100".to_owned()),
            ubr: Some(2894),
            display_version: Some("24H2".to_owned()),
        },
        process_architecture: "x86".to_owned(),
        os_architecture: Some("aarch64".to_owned()),
    }
}

/// One vector whose expected text is wrong.
const FAILING: &str = r#"
schema_version = 1

[[vector]]
id = "deliberately-wrong"
rows = ["A1"]
evidence = ["baseline"]
source = "crates/raumakey-engine/tests/matrix.rs::a01_telex_double_a_makes_circumflex"
method = "telex"
keys = "aa"
text = "a"
"#;

fn embedded_count() -> usize {
    vectors::load_embedded().map_or(0, |vectors| vectors.len())
}

fn runner_exe() -> &'static Path {
    Path::new(env!("CARGO_BIN_EXE_raumakey-test-runner"))
}

fn run_exe(args: &[&str]) -> Output {
    Process::new(runner_exe())
        .args(args)
        .output()
        .unwrap_or_else(|error| panic!("the runner must start: {error}"))
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

#[test]
fn vectors_runs_without_a_report_by_default() {
    assert_eq!(parse(&["vectors"]), Ok(Command::Vectors { json: None }));
}

#[test]
fn json_names_the_report_path() {
    assert_eq!(
        parse(&["vectors", "--json", "evidence/vectors.json"]),
        Ok(Command::Vectors {
            json: Some(PathBuf::from("evidence/vectors.json"))
        })
    );
}

#[cfg(windows)]
#[test]
fn the_report_path_may_be_any_os_string() {
    use std::os::windows::ffi::OsStringExt;
    // An unpaired surrogate: valid in a Windows path, not UTF-8.
    let path = OsString::from_wide(&[0xD800, u16::from(b'r')]);
    let args = vec![
        OsString::from("vectors"),
        OsString::from("--json"),
        path.clone(),
    ];
    assert_eq!(
        parse_args(args),
        Ok(Command::Vectors {
            json: Some(PathBuf::from(path))
        })
    );
}

#[test]
fn usage_errors_are_typed() {
    assert_eq!(parse(&[]), Err(UsageError::NoCommand));
    assert_eq!(parse(&["vector"]), Err(UsageError::UnknownCommand));
    assert_eq!(
        parse(&["--json", "x.json"]),
        Err(UsageError::UnknownCommand)
    );
    assert_eq!(
        parse(&["vectors", "--json"]),
        Err(UsageError::MissingValue { option: "--json" })
    );
    assert_eq!(
        parse(&["vectors", "--json", ""]),
        Err(UsageError::MissingValue { option: "--json" })
    );
    assert_eq!(
        parse(&["vectors", "--json", "a.json", "--json", "b.json"]),
        Err(UsageError::DuplicateOption { option: "--json" })
    );
    assert_eq!(
        parse(&["vectors", "--html"]),
        Err(UsageError::UnexpectedArgument { position: 2 })
    );
    assert_eq!(
        parse(&["vectors", "--json", "a.json", "extra"]),
        Err(UsageError::UnexpectedArgument { position: 4 })
    );
    for error in [
        UsageError::NoCommand,
        UsageError::UnknownCommand,
        UsageError::MissingValue { option: "--json" },
        UsageError::DuplicateOption { option: "--json" },
        UsageError::UnexpectedArgument { position: 2 },
    ] {
        assert!(!error.to_string().is_empty());
    }
}

#[test]
fn live_help_and_version_are_commands() {
    assert_eq!(parse(&["live"]), Ok(Command::Live));
    assert_eq!(parse(&["live", "--app", "notepad"]), Ok(Command::Live));
    for help in ["help", "--help", "-h"] {
        assert_eq!(parse(&[help]), Ok(Command::Help), "{help}");
    }
    for version in ["--version", "-V"] {
        assert_eq!(parse(&[version]), Ok(Command::Version), "{version}");
    }
}

#[test]
fn the_report_holds_every_vector_with_the_design_fields() {
    let report = match runner::run_vectors(fake_environment()) {
        Ok(report) => report,
        Err(error) => panic!("the embedded fixtures must load: {error}"),
    };
    assert_eq!(report.exit_code(), 0);
    let json = serde_json::to_value(&report).unwrap_or_default();
    assert_eq!(json["schema_version"], 1);
    assert_eq!(json["runner"]["name"], "raumakey-test-runner");
    assert_eq!(json["runner"]["version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(json["mode"], "vectors");
    let environment = &json["environment"];
    assert_eq!(environment["os"]["name"], "windows");
    assert_eq!(environment["os"]["build"], "26100");
    assert_eq!(environment["os"]["ubr"], 2894);
    assert_eq!(environment["os"]["display_version"], "24H2");
    assert_eq!(environment["process_architecture"], "x86");
    assert_eq!(environment["os_architecture"], "aarch64");
    assert_eq!(json["config_revision"]["defaults_version"], 1);
    assert!(json["config_revision"]["store_revision"].is_null());
    let fixtures = json["fixtures"].as_array().cloned().unwrap_or_default();
    assert_eq!(fixtures.len(), vectors::EMBEDDED.len());
    for fixture in &fixtures {
        assert_eq!(fixture["sha256"].as_str().map(str::len), Some(64));
    }

    let results = json["results"].as_array().cloned().unwrap_or_default();
    assert_eq!(results.len(), embedded_count());
    assert!(!results.is_empty());
    assert_eq!(json["summary"]["total"], results.len());
    assert_eq!(json["summary"]["passed"], results.len());
    assert_eq!(json["summary"]["failed"], 0);
    for result in &results {
        let id = result["id"].as_str().unwrap_or_default();
        assert!(!id.is_empty());
        // Design §15.3 and the matrix pass criteria.
        for field in [
            "rows",
            "evidence",
            "source",
            "os",
            "architecture",
            "application",
            "config_revision",
            "options",
            "keys",
            "transactions",
            "final_text",
            "status",
        ] {
            assert!(result.get(field).is_some(), "{id}: no {field}");
        }
        // The runner fills in the OS and its build; the engine ran in this process.
        assert_eq!(result["os"], environment["os"], "{id}");
        assert_eq!(result["architecture"], std::env::consts::ARCH, "{id}");
        assert_eq!(
            result["application"]["name"],
            raumakey_engine::script::SCRIPT_EXE,
            "{id}"
        );
        assert_eq!(
            result["application"]["version"],
            env!("CARGO_PKG_VERSION"),
            "{id}"
        );
        assert_eq!(result["config_revision"], json["config_revision"], "{id}");
        assert_eq!(result["status"], "pass", "{id}");
        assert_eq!(result["failures"], Value::Array(Vec::new()), "{id}");
        assert!(
            result["transactions"]
                .as_array()
                .is_some_and(|transactions| !transactions.is_empty()),
            "{id}: no transaction sequence"
        );
        assert!(result["final_text"].is_string(), "{id}");
    }
}

#[test]
fn a_failing_vector_fails_the_report() {
    let parsed = match vectors::parse_fixture("failing.toml", FAILING) {
        Ok(parsed) => parsed,
        Err(error) => panic!("the failing fixture is well-formed: {error}"),
    };
    let results: Vec<_> = parsed.iter().map(vectors::run_vector).collect();
    assert_eq!(results[0].status, Status::Fail);
    let report = Report::new(fake_environment(), &[], results);
    assert_eq!(report.summary.total, 1);
    assert_eq!(report.summary.failed, 1);
    assert_eq!(report.exit_code(), 1);
    let line = report.summary_line();
    assert!(line.contains("1 failed"), "{line}");
    assert!(!line.contains('\n'));
    let json = serde_json::to_value(&report).unwrap_or_default();
    let failures = &json["results"][0]["failures"];
    assert_eq!(failures[0]["kind"], "final-text");
    assert_eq!(failures[0]["expected"], "a");
    assert_eq!(failures[0]["actual"], "â");
}

#[test]
fn a_run_without_vectors_is_not_a_pass() {
    let report = Report::new(fake_environment(), &[], Vec::new());
    assert_eq!(report.exit_code(), 1);
}

#[test]
fn the_probed_environment_names_this_process() {
    let environment = runner::probe_environment();
    assert_eq!(environment.process_architecture, std::env::consts::ARCH);
    assert_eq!(environment.os.name, std::env::consts::OS);
    if cfg!(windows) {
        // Windows 10 and later always have CurrentBuild, UBR, and IsWow64Process2.
        let build = environment.os.build.clone().unwrap_or_default();
        let ubr = environment.os.ubr.unwrap_or_default();
        assert!(!build.is_empty(), "CurrentBuild");
        assert!(environment.os_architecture.is_some());
        // `ver` reports the same build from another source: "[Version 10.0.<build>.<ubr>]".
        let ver = Process::new("cmd")
            .args(["/d", "/c", "ver"])
            .output()
            .map(|output| text(&output.stdout))
            .unwrap_or_default();
        assert!(ver.contains(&format!(".{build}.{ubr}]")), "{ver}");
    }
}

#[test]
fn the_binary_runs_every_vector_and_writes_the_report() {
    let directory = match tempfile::tempdir() {
        Ok(directory) => directory,
        Err(error) => panic!("temporary directory: {error}"),
    };
    let path = directory.path().join("vectors.json");
    let output = Process::new(runner_exe())
        .arg("vectors")
        .arg("--json")
        .arg(&path)
        .output()
        .unwrap_or_else(|error| panic!("the runner must start: {error}"));
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    let stdout = text(&output.stdout);
    assert_eq!(stdout.lines().count(), 1, "one summary line: {stdout}");
    assert!(stdout.contains("0 failed"), "{stdout}");
    let report = fs::read_to_string(&path).unwrap_or_default();
    let json: Value = serde_json::from_str(&report).unwrap_or_default();
    assert_eq!(
        json["results"].as_array().map(Vec::len),
        Some(embedded_count())
    );
    assert_eq!(json["summary"]["failed"], 0);
    assert_eq!(
        json["environment"]["process_architecture"],
        std::env::consts::ARCH
    );
    // Vector data only: neither the report path nor the working directory.
    for location in [
        directory.path().to_path_buf(),
        std::env::current_dir().unwrap_or_default(),
    ] {
        let raw = location.to_string_lossy().into_owned();
        let escaped = serde_json::to_string(&raw).unwrap_or_default();
        assert!(!report.contains(&raw), "the report names a local path");
        assert!(
            !report.contains(escaped.trim_matches('"')),
            "the report names a local path"
        );
    }
}

#[test]
fn the_binary_prints_one_line_without_a_report() {
    let output = run_exe(&["vectors"]);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    assert_eq!(text(&output.stdout).lines().count(), 1);
    assert!(output.stderr.is_empty(), "{}", text(&output.stderr));
}

#[test]
fn live_mode_says_that_it_is_not_implemented() {
    let output = run_exe(&["live", "--app", "notepad"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert!(
        text(&output.stderr).contains("not implemented"),
        "{}",
        text(&output.stderr)
    );
}

#[test]
fn usage_errors_exit_with_2() {
    for args in [
        &[][..],
        &["bogus"],
        &["vectors", "--json"],
        &["vectors", "--html"],
    ] {
        let output = run_exe(args);
        assert_eq!(output.status.code(), Some(2), "{args:?}");
        assert!(!output.stderr.is_empty(), "{args:?}");
    }
}

#[test]
fn help_and_version_exit_with_0() {
    let help = run_exe(&["--help"]);
    assert_eq!(help.status.code(), Some(0));
    assert!(text(&help.stdout).contains("vectors"));
    let version = run_exe(&["--version"]);
    assert_eq!(version.status.code(), Some(0));
    assert_eq!(
        text(&version.stdout).trim(),
        format!("raumakey-test-runner {}", env!("CARGO_PKG_VERSION"))
    );
}

/// `text` as UTF-16LE bytes, the encoding of version-resource strings.
fn utf16le(text: &str) -> Vec<u8> {
    text.encode_utf16().flat_map(u16::to_le_bytes).collect()
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

/// `build.rs` links `raumakey-resources`' `.res` into the runner binary only (Windows MSVC).
/// This checks the bytes of the files; `(Get-Item <exe>).VersionInfo` is the Windows view.
#[cfg(all(windows, target_env = "msvc"))]
#[test]
fn only_the_runner_binary_carries_the_windows_resources() {
    let runner = fs::read(runner_exe()).unwrap_or_default();
    // VS_VERSIONINFO strings: FileDescription and OriginalFilename.
    let description = utf16le("Raumakey acceptance test runner");
    let original_filename = utf16le("raumakey-test-runner.exe");
    // The manifest's assembly identity, built at run time so that this test binary does not
    // hold it as a literal.
    let identity: String = "rennuRtseT.yekamuaR".chars().rev().collect();
    assert!(contains(&runner, &description), "no FileDescription");
    assert!(contains(&runner, &original_filename), "no OriginalFilename");
    assert!(contains(&runner, identity.as_bytes()), "no manifest");
    let this_test = std::env::current_exe()
        .and_then(fs::read)
        .unwrap_or_default();
    assert!(!this_test.is_empty());
    assert!(
        !contains(&this_test, &description),
        "a test binary has the runner's version"
    );
    assert!(
        !contains(&this_test, identity.as_bytes()),
        "a test binary has the runner's manifest"
    );
}

#[test]
fn an_unwritable_report_exits_with_1() {
    let directory = match tempfile::tempdir() {
        Ok(directory) => directory,
        Err(error) => panic!("temporary directory: {error}"),
    };
    let path = directory.path().join("missing").join("vectors.json");
    let output = Process::new(runner_exe())
        .arg("vectors")
        .arg("--json")
        .arg(&path)
        .output()
        .unwrap_or_else(|error| panic!("the runner must start: {error}"));
    assert_eq!(output.status.code(), Some(1));
    assert!(
        text(&output.stderr).contains("report"),
        "{}",
        text(&output.stderr)
    );
    assert!(!path.exists());
}
