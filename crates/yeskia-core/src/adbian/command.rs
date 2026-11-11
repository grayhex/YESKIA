use crate::{Error, Result};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Risk {
    Read,
    Write,
    Destructive,
}

/// A validated phone command. Debug intentionally excludes arguments/paths.
#[derive(Clone)]
pub struct Command {
    line: String,
    verb: String,
    risk: Risk,
}
impl std::fmt::Debug for Command {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Command")
            .field("verb", &self.verb)
            .field("risk", &self.risk)
            .finish()
    }
}
fn bad() -> Error {
    Error::InvalidArgument("ADBIAN_COMMAND_SYNTAX".into())
}
fn number(s: &str) -> bool {
    s.strip_prefix("0x")
        .map(|v| u32::from_str_radix(v, 16).is_ok())
        .unwrap_or_else(|| s.parse::<u32>().is_ok())
}
impl Command {
    pub fn parse(line: &str) -> Result<Self> {
        let line = line.trim();
        if line.is_empty() || line.len() > 4096 || line.chars().any(|c| c.is_control()) {
            return Err(bad());
        }
        let (verb, arg) = line.split_once(' ').unwrap_or((line, ""));
        let arg = arg.trim();
        let words: Vec<_> = arg.split_whitespace().collect();
        let risk = match verb {
            "ping" | "pwd" if arg.is_empty() => Risk::Read,
            "quit" if arg.is_empty() => Risk::Destructive,
            "ls" | "cd" => Risk::Read,
            "stat" | "cat" | "get" if !arg.is_empty() => Risk::Read,
            "find" | "grep" if words.len() >= 2 => Risk::Read,
            "ps" | "cenrep" if words.len() == 2 && words.iter().all(|v| number(v)) => Risk::Read,
            "hal" if arg.is_empty() || (words.len() == 1 && number(arg)) => Risk::Read,
            "mkdir" if !arg.is_empty() => Risk::Write,
            "rm" | "exec" if !arg.is_empty() => Risk::Destructive,
            "reboot" if arg == "now" => Risk::Destructive,
            // put has a second binary phase: it must go through Session::put, never raw input.
            _ => return Err(bad()),
        };
        Ok(Self {
            line: line.into(),
            verb: verb.into(),
            risk,
        })
    }
    pub fn line(&self) -> &str {
        &self.line
    }
    pub fn verb(&self) -> &str {
        &self.verb
    }
    pub fn risk(&self) -> Risk {
        self.risk
    }
    pub fn changes_device(&self) -> bool {
        self.risk != Risk::Read
    }
}
