use clap::Parser;
use cyoa_presentation::commands::Cli;
const ID: &str = "harbour-0123456789abcdef0123456789abcdef";
#[test]
fn persistence_cli_rejects_missing_policy_paths_invalid_ids_and_page_bounds() {
    for args in [
        vec!["cyoa", "play", "--headless", "--demo", "--load", ID],
        vec![
            "cyoa",
            "play",
            "--headless",
            "--demo",
            "--limits",
            "original",
        ],
        vec!["cyoa", "play", "--headless", "--demo", "--backup"],
        vec![
            "cyoa",
            "play",
            "--headless",
            "--demo",
            "--load",
            "../story",
            "--limits",
            "original",
        ],
        vec![
            "cyoa",
            "play",
            "--headless",
            "--demo",
            "--load",
            ID,
            "--limits",
            "implicit",
        ],
        vec!["cyoa", "--data-dir", "relative", "list"],
        vec!["cyoa", "inspect", "../story"],
        vec!["cyoa", "list", "--limit", "0"],
        vec!["cyoa", "list", "--limit", "101"],
        vec!["cyoa", "list", "--after", "invalid"],
    ] {
        assert!(
            Cli::try_parse_from(&args).is_err(),
            "invalid syntax admitted: {args:?}"
        );
    }
}
#[test]
fn persistence_cli_accepts_explicit_resume_policies_and_auth_free_queries() {
    for args in [
        vec![
            "cyoa",
            "--data-dir",
            "/tmp/cyoa-command-test",
            "list",
            "--limit",
            "100",
        ],
        vec!["cyoa", "list", "--after", ID],
        vec!["cyoa", "inspect", ID],
        vec!["cyoa", "inspect", ID, "--backup"],
        vec![
            "cyoa",
            "play",
            "--headless",
            "--demo",
            "--load",
            ID,
            "--limits",
            "current",
        ],
        vec![
            "cyoa",
            "play",
            "--headless",
            "--demo",
            "--load",
            ID,
            "--limits",
            "original",
            "--backup",
        ],
        vec![
            "cyoa",
            "play",
            "--headless",
            "--backend",
            "codex",
            "--load",
            ID,
            "--limits",
            "original",
            "--backup",
        ],
        vec![
            "cyoa",
            "play",
            "--headless",
            "--backend",
            "claude",
            "--load",
            ID,
            "--limits",
            "current",
        ],
    ] {
        assert!(
            Cli::try_parse_from(&args).is_ok(),
            "valid syntax rejected: {args:?}"
        );
    }
}

#[test]
fn in_session_persistence_syntax_rejects_partial_paths_and_invalid_rewind_before_effects() {
    use cyoa_presentation::commands::{PersistenceCommand, persistence_command};
    for command in [
        "/save unexpected",
        "/save-copy unexpected",
        "/list ../story",
        "/list a b",
        "/load",
        "/load ../story --limits original",
        "/load harbour-0123456789abcdef0123456789abcdef",
        "/load harbour-0123456789abcdef0123456789abcdef --limits implicit",
        "/load harbour-0123456789abcdef0123456789abcdef --limits current --limits original",
        "/rewind 0",
        "/rewind -1",
        "/rewind 184467440737095516160",
        "/rewind 1 2",
    ] {
        assert!(
            persistence_command(command).unwrap().is_err(),
            "invalid command admitted: {command}"
        );
    }
    assert!(matches!(
        persistence_command("/rewind 1").unwrap(),
        Ok(PersistenceCommand::Rewind(_))
    ));
    assert!(matches!(
        persistence_command(&format!("/load {ID} --limits original --backup")).unwrap(),
        Ok(PersistenceCommand::Load(_))
    ));
    assert!(persistence_command("//load literal text").is_none());
}
