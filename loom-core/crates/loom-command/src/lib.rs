//! `loom-command` maps every user action to a command identifier with
//! enablement/checked state and an undo description. This is the spine for
//! menus, shortcuts, command palette, and plugin invocation.

use std::collections::HashMap;
use std::fmt;

/// Stable command identifier, e.g. `"writer.format.bold"`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CommandId(pub String);

impl CommandId {
    /// Create a command id.
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }
}

impl fmt::Display for CommandId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<&str> for CommandId {
    fn from(s: &str) -> Self {
        Self::new(s)
    }
}

impl From<String> for CommandId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

/// Context in which a command is invoked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InvocationSource {
    /// Menu item.
    Menu,
    /// Keyboard shortcut.
    Shortcut,
    /// Command palette.
    Palette,
    /// Context menu.
    ContextMenu,
    /// Accessibility API.
    Accessibility,
    /// Script or plugin.
    Plugin,
    /// Test harness.
    Test,
}

/// Required properties of a command.
#[derive(Debug, Clone)]
pub struct CommandSpec {
    /// Unique id.
    pub id: CommandId,
    /// Short, localized label.
    pub label: String,
    /// Undo description.
    pub undo_label: String,
    /// Whether command is currently enabled.
    pub enabled: bool,
    /// Whether command is checked (for toggle commands).
    pub checked: bool,
    /// Default shortcut suggestion (may be localized/reassigned).
    pub default_shortcut: Option<String>,
    /// Group for command palette organization.
    pub category: String,
}

impl CommandSpec {
    /// Builder convenience.
    pub fn new(id: impl Into<CommandId>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            undo_label: String::new(),
            enabled: true,
            checked: false,
            default_shortcut: None,
            category: "general".into(),
        }
    }
}

/// Registry of registered command specs.
#[derive(Debug, Default)]
pub struct CommandRegistry {
    commands: HashMap<CommandId, CommandSpec>,
}

impl CommandRegistry {
    /// New registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a command spec.
    pub fn register(&mut self, spec: CommandSpec) {
        self.commands.insert(spec.id.clone(), spec);
    }

    /// Look up by id.
    pub fn get(&self, id: &CommandId) -> Option<&CommandSpec> {
        self.commands.get(id)
    }

    /// Iterate all commands.
    pub fn commands(&self) -> impl Iterator<Item = &CommandSpec> {
        self.commands.values()
    }

    /// Number of registered commands.
    pub fn len(&self) -> usize {
        self.commands.len()
    }

    /// Whether empty.
    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }

    /// Search commands by fuzzy substring on label/id/category.
    pub fn search(&self, query: &str) -> Vec<(&CommandSpec, u32)> {
        if query.trim().is_empty() {
            return Vec::new();
        }
        let q = query.to_lowercase();
        let mut results: Vec<(&CommandSpec, u32)> = self
            .commands
            .values()
            .map(|spec| {
                let mut score = 0u32;
                if spec.label.to_lowercase().contains(&q) {
                    score += 3;
                }
                if spec.id.0.to_lowercase().contains(&q) {
                    score += 2;
                }
                if spec.category.to_lowercase().contains(&q) {
                    score += 1;
                }
                (spec, score)
            })
            .filter(|(_, s)| *s > 0)
            .collect();
        results.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.id.cmp(&b.0.id)));
        results
    }
}

/// A command invocation request.
#[derive(Debug, Clone)]
pub struct CommandInvocation {
    /// Command id.
    pub id: CommandId,
    /// Source.
    pub source: InvocationSource,
    /// Optional arguments.
    pub args: Option<Vec<u8>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_registry() -> CommandRegistry {
        let mut r = CommandRegistry::new();
        r.register(CommandSpec::new("file.save", "Save"));
        r.register(CommandSpec::new("edit.undo", "Undo"));
        r.register(CommandSpec::new("edit.redo", "Redo"));
        r
    }

    #[test]
    fn register_and_lookup() {
        let r = sample_registry();
        assert!(r.get(&CommandId::new("file.save")).is_some());
        assert!(r.get(&CommandId::new("missing")).is_none());
        assert_eq!(r.len(), 3);
    }

    #[test]
    fn search_matches() {
        let r = sample_registry();
        let res = r.search("undo");
        assert_eq!(res.len(), 1);
        assert_eq!(res[0].0.id.0, "edit.undo");
        let res2 = r.search("save");
        assert_eq!(res2.len(), 1);
    }

    #[test]
    fn search_scores_rank() {
        let mut r = CommandRegistry::new();
        r.register(CommandSpec::new("file.save", "Save File"));
        r.register(CommandSpec::new("edit.savearea", "Format Area"));
        let res = r.search("save");
        // label match ranks above id partial.
        assert_eq!(res[0].0.id.0, "file.save");
    }

    #[test]
    fn empty_query_returns_nothing() {
        let r = sample_registry();
        assert!(r.search("").is_empty());
    }

    #[test]
    fn enablement_state() {
        let mut r = CommandRegistry::new();
        let mut spec = CommandSpec::new("edit.undo", "Undo");
        spec.enabled = false;
        r.register(spec);
        assert!(!r.get(&CommandId::new("edit.undo")).unwrap().enabled);
    }
}
