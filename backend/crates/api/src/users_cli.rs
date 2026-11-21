//! The `teach-me user …` subcommands (`docs/contracts/users.md` §5): the way
//! an operator manages users from the server. They talk to the database
//! directly through the same use cases the API uses.
//!
//! Secrets never appear in the output, except the generated password of
//! `user add` / `user passwd`, printed once.

use std::io::{BufRead, IsTerminal, Write};
use std::sync::Arc;

use application::{AppError, CreateUser, NewAccount, UserAdmin};
use domain::{Clock, DocumentFiles, IdGenerator, User};
use infrastructure::sqlite::Pool;
use infrastructure::{SqliteDocumentRepository, SqliteSessionRepository, SqliteUserRepository};

use crate::state::AuthSetup;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UserCommand {
    Add {
        username: String,
        display_name: Option<String>,
    },
    List,
    Passwd {
        username: String,
    },
    Rename {
        username: String,
        new_name: String,
    },
    SetLlmKey {
        username: String,
    },
    Disable {
        username: String,
    },
    Enable {
        username: String,
    },
    Delete {
        username: String,
        /// Skips the confirmation prompt (scripts).
        yes: bool,
    },
}

pub const USAGE: &str = "usage:
  teach-me user add <username> [--display-name <name>]
  teach-me user list
  teach-me user passwd <username>
  teach-me user rename <username> <new-username>
  teach-me user set-llm-key <username>
  teach-me user disable <username>
  teach-me user enable <username>
  teach-me user delete <username> [--yes]

Passwords and keys are read from standard input (one line); without input
`add` and `passwd` generate a temporary password and print it once.";

/// `args` are the words after `user`.
pub fn parse(args: &[String]) -> Result<UserCommand, String> {
    let (cmd, rest) = args.split_first().ok_or("missing the command")?;
    let one = |rest: &[String]| -> Result<String, String> {
        match rest {
            [name] => Ok(name.clone()),
            _ => Err(format!("`{cmd}` takes exactly one username")),
        }
    };
    match cmd.as_str() {
        "add" => {
            let (name, flags) = rest.split_first().ok_or("`add` needs a username")?;
            let display_name = match flags {
                [] => None,
                [flag, value] if flag == "--display-name" => Some(value.clone()),
                _ => {
                    return Err(
                        "`add` takes only `--display-name <name>` after the username".into(),
                    );
                }
            };
            Ok(UserCommand::Add {
                username: name.clone(),
                display_name,
            })
        }
        "list" if rest.is_empty() => Ok(UserCommand::List),
        "list" => Err("`list` takes no arguments".into()),
        "passwd" => Ok(UserCommand::Passwd {
            username: one(rest)?,
        }),
        "rename" => match rest {
            [name, new_name] => Ok(UserCommand::Rename {
                username: name.clone(),
                new_name: new_name.clone(),
            }),
            _ => Err("`rename` takes the username and the new one".into()),
        },
        "set-llm-key" => Ok(UserCommand::SetLlmKey {
            username: one(rest)?,
        }),
        "disable" => Ok(UserCommand::Disable {
            username: one(rest)?,
        }),
        "enable" => Ok(UserCommand::Enable {
            username: one(rest)?,
        }),
        "delete" => {
            let yes = rest.iter().any(|a| a == "--yes");
            let names: Vec<String> = rest.iter().filter(|a| *a != "--yes").cloned().collect();
            Ok(UserCommand::Delete {
                username: one(&names)?,
                yes,
            })
        }
        other => Err(format!("unknown command `{other}`")),
    }
}

/// What the commands need from the terminal; a fake in tests.
pub trait Console {
    /// One line of piped input, without the line end, for secrets: `None`
    /// when there is none (standard input is a terminal, whose echo is not
    /// hidden here, or it is at its end).
    fn read_piped(&mut self) -> Option<String>;
    /// One line typed or piped, for a confirmation.
    fn read_line(&mut self) -> Option<String>;
    fn println(&mut self, line: &str);
}

/// Standard input and output. A terminal counts as "no input": nothing is
/// echo-protected here, so secrets are not typed into it.
pub struct StdConsole;

impl Console for StdConsole {
    fn read_piped(&mut self) -> Option<String> {
        if std::io::stdin().is_terminal() {
            return None;
        }
        self.read_line()
    }

    fn read_line(&mut self) -> Option<String> {
        let mut line = String::new();
        std::io::stdin().lock().read_line(&mut line).ok()?;
        Some(line.trim_end_matches(['\r', '\n']).to_owned())
    }

    fn println(&mut self, line: &str) {
        let mut out = std::io::stdout().lock();
        let _ = writeln!(out, "{line}");
    }
}

pub struct AdminTools {
    pub create: CreateUser,
    pub admin: UserAdmin,
}

impl AdminTools {
    #[must_use]
    pub fn new(
        pool: &Pool,
        auth: &AuthSetup,
        files: Arc<dyn DocumentFiles>,
        clock: Arc<dyn Clock>,
        ids: Arc<dyn IdGenerator>,
    ) -> Self {
        let users = Arc::new(SqliteUserRepository::new(pool.clone()));
        let sessions = Arc::new(SqliteSessionRepository::new(pool.clone()));
        Self {
            create: CreateUser::new(
                users.clone(),
                auth.hasher.clone(),
                auth.tokens.clone(),
                clock.clone(),
                ids,
            ),
            admin: UserAdmin::new(
                users,
                sessions,
                Arc::new(SqliteDocumentRepository::new(pool.clone())),
                files,
                auth.hasher.clone(),
                auth.tokens.clone(),
                auth.cipher.clone(),
                clock,
            ),
        }
    }
}

/// Runs a command. The error text is for the operator and carries no secret.
pub async fn run(
    command: UserCommand,
    tools: &AdminTools,
    console: &mut dyn Console,
) -> Result<(), String> {
    match command {
        UserCommand::Add {
            username,
            display_name,
        } => {
            let password = console.read_piped().filter(|p| !p.is_empty());
            let created = tools
                .create
                .execute(NewAccount {
                    username,
                    display_name,
                    password,
                })
                .await
                .map_err(explain)?;
            console.println(&format!("Created user `{}`.", created.user.username));
            if let Some(generated) = created.generated_password {
                console.println(&format!(
                    "Temporary password (shown once): {}",
                    generated.expose()
                ));
            }
        }
        UserCommand::List => {
            let users = tools.admin.list().await.map_err(explain)?;
            console.println("USERNAME\tNAME\tSTATE\tLLM KEY\tLAST LOGIN");
            for user in &users {
                console.println(&row(user));
            }
        }
        UserCommand::Passwd { username } => {
            let password = console.read_piped().filter(|p| !p.is_empty());
            let done = tools
                .admin
                .set_password(&username, password.as_deref())
                .await
                .map_err(explain)?;
            console.println("Password changed; every session of the user was revoked.");
            if let Some(generated) = done.generated {
                console.println(&format!(
                    "Temporary password (shown once): {}",
                    generated.expose()
                ));
            }
        }
        UserCommand::Rename { username, new_name } => {
            tools
                .admin
                .rename(&username, &new_name)
                .await
                .map_err(explain)?;
            console.println("Renamed.");
        }
        UserCommand::SetLlmKey { username } => {
            let key = console
                .read_piped()
                .filter(|k| !k.trim().is_empty())
                .ok_or("the key is read from standard input: pipe it in, e.g. `printf %s \"$KEY\" | teach-me user set-llm-key <username>`")?;
            tools
                .admin
                .set_llm_key(&username, &key)
                .await
                .map_err(explain)?;
            console.println("LLM key stored.");
        }
        UserCommand::Disable { username } => {
            tools
                .admin
                .set_disabled(&username, true)
                .await
                .map_err(explain)?;
            console.println("Disabled; the user's sessions were revoked.");
        }
        UserCommand::Enable { username } => {
            tools
                .admin
                .set_disabled(&username, false)
                .await
                .map_err(explain)?;
            console.println("Enabled.");
        }
        UserCommand::Delete { username, yes } => {
            if !yes {
                console.println(&format!(
                    "This deletes `{username}` and ALL their data, files included. \
                     Type the username to confirm:"
                ));
                let typed = console.read_line().unwrap_or_default();
                if typed.trim().to_lowercase() != username.trim().to_lowercase() {
                    return Err("not confirmed: nothing was deleted (use --yes in scripts)".into());
                }
            }
            let files = tools.admin.delete(&username).await.map_err(explain)?;
            console.println(&format!("Deleted; {files} uploaded file(s) removed."));
        }
    }
    Ok(())
}

fn row(user: &User) -> String {
    let state = if user.is_active() {
        "active"
    } else {
        "disabled"
    };
    let key = if user.has_llm_key { "yes" } else { "no" };
    let last = user
        .last_login_at
        .map_or_else(|| "never".to_owned(), crate::health::format_instant);
    format!(
        "{}\t{}\t{state}\t{key}\t{last}",
        user.username,
        user.display_name.as_str()
    )
}

fn explain(error: AppError) -> String {
    match error {
        AppError::NotFound => "no such user".into(),
        AppError::Validation(issues) => {
            let list: Vec<String> = issues
                .iter()
                .map(|i| format!("{}: {}", i.field, i.issue.as_str()))
                .collect();
            format!("invalid input ({})", list.join(", "))
        }
        other => format!("failed: {other}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use infrastructure::{
        Argon2idHasher, LocalDocumentFiles, OsTokenIssuer, ServerKey, SystemClock, UuidV7Generator,
        XChaChaSecretCipher,
    };
    use std::collections::VecDeque;

    #[derive(Default)]
    struct FakeConsole {
        piped: VecDeque<String>,
        typed: VecDeque<String>,
        out: Vec<String>,
    }

    impl Console for FakeConsole {
        fn read_piped(&mut self) -> Option<String> {
            self.piped.pop_front()
        }
        fn read_line(&mut self) -> Option<String> {
            self.typed.pop_front()
        }
        fn println(&mut self, line: &str) {
            self.out.push(line.to_owned());
        }
    }

    fn words(line: &str) -> Vec<String> {
        line.split_whitespace().map(str::to_owned).collect()
    }

    async fn tools() -> (AdminTools, Pool, std::path::PathBuf) {
        let pool = infrastructure::sqlite::connect("sqlite::memory:")
            .await
            .unwrap();
        infrastructure::sqlite::migrate(&pool).await.unwrap();
        let dir = std::env::temp_dir().join(format!("teach_me_cli_{}", uuid::Uuid::now_v7()));
        let files = Arc::new(LocalDocumentFiles::new(dir.join("uploads")));
        files.prepare().await.unwrap();
        let auth = AuthSetup {
            hasher: Arc::new(Argon2idHasher::with_costs(8, 1, 1).unwrap()),
            cipher: Arc::new(XChaChaSecretCipher::new(&ServerKey::from_bytes([1; 32])).unwrap()),
            tokens: Arc::new(OsTokenIssuer),
            llm_provider: "anthropic".into(),
        };
        let tools = AdminTools::new(
            &pool,
            &auth,
            files,
            Arc::new(SystemClock),
            Arc::new(UuidV7Generator),
        );
        (tools, pool, dir)
    }

    async fn run_line(
        tools: &AdminTools,
        console: &mut FakeConsole,
        line: &str,
    ) -> Result<(), String> {
        run(parse(&words(line)).unwrap(), tools, console).await
    }

    #[test]
    fn the_commands_are_parsed_and_misuse_is_reported() {
        assert_eq!(
            parse(&words("add ann --display-name Ann")).unwrap(),
            UserCommand::Add {
                username: "ann".into(),
                display_name: Some("Ann".into())
            }
        );
        assert_eq!(parse(&words("list")).unwrap(), UserCommand::List);
        assert_eq!(
            parse(&words("rename ann anna")).unwrap(),
            UserCommand::Rename {
                username: "ann".into(),
                new_name: "anna".into()
            }
        );
        assert_eq!(
            parse(&words("delete ann --yes")).unwrap(),
            UserCommand::Delete {
                username: "ann".into(),
                yes: true
            }
        );
        for bad in [
            "",
            "add",
            "add a b c",
            "passwd",
            "passwd a b",
            "list x",
            "frobnicate x",
            "rename a",
        ] {
            assert!(parse(&words(bad)).is_err(), "{bad}");
        }
    }

    #[tokio::test]
    async fn add_reads_the_password_from_input_or_generates_one_shown_once() {
        let (tools, pool, dir) = tools().await;
        let mut c = FakeConsole::default();
        c.piped.push_back("my own long password".into());
        run_line(&tools, &mut c, "add ann").await.unwrap();
        assert_eq!(
            c.out,
            ["Created user `ann`."],
            "a password given is not echoed"
        );

        let mut c = FakeConsole::default();
        run_line(&tools, &mut c, "add bob --display-name Bob")
            .await
            .unwrap();
        assert_eq!(c.out.len(), 2);
        let shown = c.out[1]
            .strip_prefix("Temporary password (shown once): ")
            .unwrap();
        assert!(shown.len() >= 10);
        // ...and it is the one that was stored (only its hash is).
        let stored: String =
            sqlx::query_scalar("SELECT password_hash FROM users WHERE username = 'bob'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(stored.starts_with("$argon2id$") && !stored.contains(shown));

        let err = run_line(&tools, &mut FakeConsole::default(), "add ANN")
            .await
            .unwrap_err();
        assert!(err.contains("username: duplicate"), "{err}");
        std::fs::remove_dir_all(dir).ok();
    }

    #[tokio::test]
    async fn the_other_commands_do_what_they_say_without_printing_secrets() {
        let (tools, pool, dir) = tools().await;
        let mut c = FakeConsole::default();
        run_line(&tools, &mut c, "add ann").await.unwrap();

        let mut c = FakeConsole::default();
        c.piped.push_back("sk-or-never-printed".into());
        run_line(&tools, &mut c, "set-llm-key ann").await.unwrap();
        assert_eq!(c.out, ["LLM key stored."]);
        let sealed: Vec<u8> =
            sqlx::query_scalar("SELECT llm_key FROM users WHERE username = 'ann'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(!sealed.windows(5).any(|w| w == b"never"));
        let no_input = run_line(&tools, &mut FakeConsole::default(), "set-llm-key ann")
            .await
            .unwrap_err();
        assert!(no_input.contains("standard input") && !no_input.contains("sk-"));

        let mut c = FakeConsole::default();
        run_line(&tools, &mut c, "list").await.unwrap();
        let all = c.out.join("\n");
        assert!(all.contains("ann\tann\tactive\tyes\tnever"), "{all}");
        assert!(all.contains("owner\towner\tactive\tno\tnever"), "{all}");
        assert!(!all.contains("never-printed") && !all.contains("argon2"));

        let mut c = FakeConsole::default();
        run_line(&tools, &mut c, "disable ann").await.unwrap();
        run_line(&tools, &mut c, "rename ann anna").await.unwrap();
        run_line(&tools, &mut c, "enable anna").await.unwrap();
        let state: (String, Option<String>) =
            sqlx::query_as("SELECT username, disabled_at FROM users WHERE username = 'anna'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(state, ("anna".into(), None));

        let mut c = FakeConsole::default();
        c.piped.push_back("a brand new password".into());
        run_line(&tools, &mut c, "passwd owner").await.unwrap();
        let owner_hash: Option<String> =
            sqlx::query_scalar("SELECT password_hash FROM users WHERE username = 'owner'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(owner_hash.is_some());
        assert_eq!(
            run_line(&tools, &mut FakeConsole::default(), "disable ghost")
                .await
                .unwrap_err(),
            "no such user"
        );
        std::fs::remove_dir_all(dir).ok();
    }

    #[tokio::test]
    async fn delete_asks_for_the_name_and_removes_the_data_and_the_uploaded_files() {
        let (tools, pool, dir) = tools().await;
        run_line(&tools, &mut FakeConsole::default(), "add ann")
            .await
            .unwrap();
        let ann: String = sqlx::query_scalar("SELECT id FROM users WHERE username = 'ann'")
            .fetch_one(&pool)
            .await
            .unwrap();
        let at = "2026-11-10T10:00:00.000Z";
        sqlx::query("INSERT INTO folders (id, user_id, name, name_key, position, created_at, updated_at) VALUES ('f1', ?, 'F', 'f', 0, ?, ?)")
            .bind(&ann).bind(at).bind(at).execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO topics (id, user_id, folder_id, title, title_key, created_at, updated_at) VALUES ('t1', ?, 'f1', 'T', 't', ?, ?)")
            .bind(&ann).bind(at).bind(at).execute(&pool).await.unwrap();
        let doc = uuid::Uuid::now_v7();
        sqlx::query("INSERT INTO documents (id, topic_id, filename, title, status, size_bytes, created_at) VALUES (?, 't1', 'a.pdf', 'A', 'importing', 3, ?)")
            .bind(doc.to_string()).bind(at).execute(&pool).await.unwrap();
        let files = LocalDocumentFiles::new(dir.join("uploads"));
        let file = files.path(domain::DocumentId::new(doc));
        std::fs::write(&file, b"%PDF").unwrap();
        // Another user's file stays.
        let other = files.path(domain::DocumentId::new(uuid::Uuid::now_v7()));
        std::fs::write(&other, b"%PDF").unwrap();

        // Typing the wrong name stops it.
        let mut c = FakeConsole::default();
        c.typed.push_back("someone-else".into());
        let err = run_line(&tools, &mut c, "delete ann").await.unwrap_err();
        assert!(err.contains("nothing was deleted"), "{err}");
        assert!(file.exists());

        let mut c = FakeConsole::default();
        c.typed.push_back("ANN".into());
        run_line(&tools, &mut c, "delete ann").await.unwrap();
        assert_eq!(
            c.out.last().unwrap(),
            "Deleted; 1 uploaded file(s) removed."
        );
        assert!(!file.exists(), "the upload of the deleted user is gone");
        assert!(other.exists());
        for table in [
            "users WHERE username = 'ann'",
            "folders",
            "topics",
            "documents",
        ] {
            let n: i64 =
                sqlx::query_scalar(sqlx::AssertSqlSafe(format!("SELECT COUNT(*) FROM {table}")))
                    .fetch_one(&pool)
                    .await
                    .unwrap();
            assert_eq!(n, 0, "{table}");
        }
        std::fs::remove_dir_all(dir).ok();
    }
}
