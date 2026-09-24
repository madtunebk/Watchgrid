//! `watchgrid user …` — the only way to manage accounts.

use watchgrid_model::Role;
use zeroize::Zeroizing;

use super::{password, prompt, sessions, users};

pub const USAGE: &str = "\
usage: watchgrid user <command> <username>

commands:
  create <username> [--viewer]   add a user (admin unless --viewer); prompts for the password
  list                           show users
  passwd <username>              set a new password (signs the user out everywhere)
  enable <username>              allow sign-in
  disable <username>             block sign-in (signs the user out everywhere)
  delete <username>              remove the user and their sessions";

pub async fn run(db: &sqlx::PgPool, args: &[String]) -> Result<(), String> {
    let cmd = args.first().map(String::as_str);
    let name = args.get(1).map(|s| s.trim().to_lowercase());
    let need_name = || name.clone().filter(|n| users::valid_username(n)).ok_or("a username is needed: lowercase letters, digits, . _ - (max 32)".to_string());
    let db_err = |e: sqlx::Error| format!("database error: {e}");
    match cmd {
        Some("list") => {
            let all = users::all(db).await.map_err(db_err)?;
            if all.is_empty() {
                println!("No Watchgrid users exist. Create one with: sudo watchgrid user create <username>");
            }
            for u in all {
                let last = u.last_login.map_or("never".into(), |t| t.format("%Y-%m-%d %H:%M UTC").to_string());
                println!("{:<24} {:<7} {:<9} last login: {last}", u.username, users::role_name(u.role), if u.enabled { "enabled" } else { "DISABLED" });
            }
            Ok(())
        }
        Some("create") => {
            let name = need_name()?;
            if users::find(db, &name).await.map_err(db_err)?.is_some() {
                return Err(format!("user `{name}` already exists"));
            }
            let role = if args.iter().any(|a| a == "--viewer") { Role::Viewer } else { Role::Admin };
            let hash = new_password_hash()?;
            users::create(db, &name, &hash, role).await.map_err(db_err)?;
            println!("Created {} `{name}`.", users::role_name(role));
            Ok(())
        }
        Some("passwd") => {
            let name = need_name()?;
            if users::find(db, &name).await.map_err(db_err)?.is_none() {
                return Err(format!("no user `{name}`"));
            }
            let hash = new_password_hash()?;
            users::set_password(db, &name, &hash).await.map_err(db_err)?;
            let n = sessions::delete_for_user(db, &name).await.map_err(db_err)?;
            println!("Password changed for `{name}`; {n} session(s) signed out.");
            Ok(())
        }
        Some(c @ ("enable" | "disable")) => {
            let name = need_name()?;
            let enable = c == "enable";
            if !users::set_enabled(db, &name, enable).await.map_err(db_err)? {
                return Err(format!("no user `{name}`"));
            }
            if !enable {
                sessions::delete_for_user(db, &name).await.map_err(db_err)?;
            }
            println!("User `{name}` {}.", if enable { "enabled" } else { "disabled and signed out" });
            Ok(())
        }
        Some("delete") => {
            let name = need_name()?;
            if !users::delete(db, &name).await.map_err(db_err)? {
                return Err(format!("no user `{name}`"));
            }
            println!("Deleted `{name}`.");
            Ok(())
        }
        _ => Err(USAGE.into()),
    }
}

/// Ask twice on the terminal, check strength, hash; buffers are wiped on drop.
fn new_password_hash() -> Result<String, String> {
    let first: Zeroizing<String> = prompt::read_hidden("New password: ").map_err(|e| e.to_string())?;
    if first.chars().count() < password::MIN_LENGTH {
        return Err(format!("the password must be at least {} characters", password::MIN_LENGTH));
    }
    let second = prompt::read_hidden("Repeat password: ").map_err(|e| e.to_string())?;
    if *first != *second {
        return Err("the passwords do not match".into());
    }
    password::hash(&first)
}
