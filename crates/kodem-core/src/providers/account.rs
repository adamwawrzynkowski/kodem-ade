//! Process environment that binds a provider CLI to one account.
use crate::model::*;
use std::process::Command;

/// Variables that would move an added account's CLI to another credential store or would
/// override a task's own options.
///
/// No variable that selects an authentication method is ever removed: Anthropic's terms for
/// products that run Claude Code forbid removing, disabling or restricting an authentication
/// method of the CLI, including the user's own API key. When `ANTHROPIC_API_KEY` or
/// `ANTHROPIC_AUTH_TOKEN` is set, the CLI uses it for every account, and Kodem reports that.
const OVERRIDES: [(Provider, &str, Scope); 2] = [
    (
        Provider::Claude,
        "CLAUDE_SECURESTORAGE_CONFIG_DIR",
        Scope::AddedAccount,
    ),
    (
        Provider::Claude,
        "CLAUDE_CODE_EFFORT_LEVEL",
        Scope::EveryAccount,
    ),
];

// Which accounts a variable is cleared for. The default account is the CLI as the user set it
// up, so only an option that a task chooses itself is cleared there.
#[derive(Clone, Copy, PartialEq)]
enum Scope {
    AddedAccount,
    EveryAccount,
}

impl Account {
    /// Points a provider CLI at this account's configuration directory, or at its own default.
    pub fn apply(&self, command: &mut Command) {
        if let Some(variable) = self.provider.home_variable() {
            match &self.home {
                Some(home) => command.env(variable, home),
                None => command.env_remove(variable),
            };
        }
        for variable in self.overrides() {
            command.env_remove(variable);
        }
    }

    // Returns the variables that do not apply to this account's CLI processes.
    fn overrides(&self) -> impl Iterator<Item = &'static str> + '_ {
        OVERRIDES
            .iter()
            .filter(|(provider, _, scope)| {
                *provider == self.provider && (*scope == Scope::EveryAccount || self.home.is_some())
            })
            .map(|(_, variable, _)| *variable)
    }

    /// Lists the variables a sign-in shell must clear before running this account's CLI.
    pub fn cleared_variables(&self) -> Vec<&'static str> {
        let mut variables: Vec<&'static str> = self.overrides().collect();
        if self.home.is_none()
            && let Some(variable) = self.provider.home_variable()
        {
            variables.push(variable);
        }
        variables
    }
}
