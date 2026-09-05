//! `loom-command` is the authoritative command bus for Loom applications.
//! Every meaningful user action (from toolbars, menus, keyboard shortcuts,
//! context menus, command palette, accessibility actions, plugins, and test
//! harnesses) dispatches through this command registry.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

/// Stable command identifier, e.g. `"writer.format.bold"` or `"file.save"`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CommandId(pub String);

impl CommandId {
    /// Create a new command id.
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    /// Borrow the identifier string slice.
    pub fn as_str(&self) -> &str {
        &self.0
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InvocationSource {
    /// Toolbar button.
    Toolbar,
    /// Application menu item.
    Menu,
    /// Keyboard shortcut.
    Shortcut,
    /// Command palette.
    Palette,
    /// Context menu.
    ContextMenu,
    /// Accessibility API action.
    Accessibility,
    /// Script or plugin.
    Plugin,
    /// Automated test harness.
    Test,
}

impl fmt::Display for InvocationSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Toolbar => write!(f, "toolbar"),
            Self::Menu => write!(f, "menu"),
            Self::Shortcut => write!(f, "shortcut"),
            Self::Palette => write!(f, "palette"),
            Self::ContextMenu => write!(f, "context_menu"),
            Self::Accessibility => write!(f, "accessibility"),
            Self::Plugin => write!(f, "plugin"),
            Self::Test => write!(f, "test"),
        }
    }
}

/// Keyboard shortcut representation.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ShortcutSpec {
    /// Primary key description, e.g. `"S"`, `"Z"`, `"F"`.
    pub key: String,
    /// Whether Control key (or Command on macOS) is required.
    pub primary_modifier: bool,
    /// Whether Shift key is required.
    pub shift: bool,
    /// Whether Alt/Option key is required.
    pub alt: bool,
}

impl ShortcutSpec {
    /// Create a primary modifier shortcut (Ctrl on Windows/Linux, Cmd on macOS).
    pub fn primary(key: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            primary_modifier: true,
            shift: false,
            alt: false,
        }
    }

    /// Create a primary modifier + Shift shortcut (e.g. Ctrl+Shift+S).
    pub fn primary_shift(key: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            primary_modifier: true,
            shift: true,
            alt: false,
        }
    }

    /// Format for display on the current platform.
    pub fn display_string(&self) -> String {
        let mut parts = Vec::new();
        if self.primary_modifier {
            if cfg!(target_os = "macos") {
                parts.push("Cmd");
            } else {
                parts.push("Ctrl");
            }
        }
        if self.alt {
            if cfg!(target_os = "macos") {
                parts.push("Opt");
            } else {
                parts.push("Alt");
            }
        }
        if self.shift {
            parts.push("Shift");
        }
        parts.push(self.key.as_str());
        parts.join("+")
    }
}

/// Structured outcome of a successful command execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandOutcome {
    /// Executed command id.
    pub id: CommandId,
    /// Human-readable message or notification.
    pub message: Option<String>,
    /// Opaque history payload (if the command produced an undoable delta).
    pub payload: Option<Vec<u8>>,
    /// Regions of the document or canvas invalidated by this command.
    pub invalidated_regions: Vec<String>,
    /// Screen reader or accessibility announcements.
    pub announcements: Vec<String>,
}

impl CommandOutcome {
    /// Create a simple success outcome for a command.
    pub fn success(id: impl Into<CommandId>) -> Self {
        Self {
            id: id.into(),
            message: None,
            payload: None,
            invalidated_regions: Vec::new(),
            announcements: Vec::new(),
        }
    }

    /// Attach an announcement for accessibility.
    pub fn with_announcement(mut self, text: impl Into<String>) -> Self {
        self.announcements.push(text.into());
        self
    }

    /// Attach a status message.
    pub fn with_message(mut self, msg: impl Into<String>) -> Self {
        self.message = Some(msg.into());
        self
    }

    /// Attach an invalidation region identifier.
    pub fn with_invalidation(mut self, region: impl Into<String>) -> Self {
        self.invalidated_regions.push(region.into());
        self
    }

    /// Attach history payload data.
    pub fn with_payload(mut self, payload: Vec<u8>) -> Self {
        self.payload = Some(payload);
        self
    }
}

/// Structured errors returned by command invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandError {
    /// Command is currently disabled and cannot be executed.
    Disabled(CommandId),
    /// Command is not registered in the registry.
    NotFound(CommandId),
    /// Command was cancelled by the user or preflight check.
    Cancelled,
    /// Invalid arguments provided to the command.
    InvalidArguments(String),
    /// Execution failed with an application error.
    ExecutionFailed(String),
}

impl fmt::Display for CommandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Disabled(id) => write!(f, "command '{id}' is currently disabled"),
            Self::NotFound(id) => write!(f, "command '{id}' not found in registry"),
            Self::Cancelled => write!(f, "command execution cancelled"),
            Self::InvalidArguments(err) => write!(f, "invalid command arguments: {err}"),
            Self::ExecutionFailed(err) => write!(f, "command execution failed: {err}"),
        }
    }
}

impl std::error::Error for CommandError {}

/// A command invocation request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandInvocation {
    /// Command id to invoke.
    pub id: CommandId,
    /// Invocation source (menu, shortcut, palette, etc.).
    pub source: InvocationSource,
    /// Optional arbitrary payload arguments.
    pub args: Option<Vec<u8>>,
}

impl CommandInvocation {
    /// Create a new invocation request.
    pub fn new(id: impl Into<CommandId>, source: InvocationSource) -> Self {
        Self {
            id: id.into(),
            source,
            args: None,
        }
    }

    /// Create an invocation with arguments.
    pub fn with_args(id: impl Into<CommandId>, source: InvocationSource, args: Vec<u8>) -> Self {
        Self {
            id: id.into(),
            source,
            args: Some(args),
        }
    }
}

/// Trait implemented by command handlers.
pub trait CommandHandler: Send + Sync {
    /// Execute the command invocation.
    fn execute(&self, invocation: &CommandInvocation) -> Result<CommandOutcome, CommandError>;
}

/// Function-based command handler.
pub struct FnCommandHandler<F> {
    func: F,
}

impl<F> FnCommandHandler<F>
where
    F: Fn(&CommandInvocation) -> Result<CommandOutcome, CommandError> + Send + Sync,
{
    /// Create a handler from a closure.
    pub fn new(func: F) -> Self {
        Self { func }
    }
}

impl<F> CommandHandler for FnCommandHandler<F>
where
    F: Fn(&CommandInvocation) -> Result<CommandOutcome, CommandError> + Send + Sync,
{
    fn execute(&self, invocation: &CommandInvocation) -> Result<CommandOutcome, CommandError> {
        (self.func)(invocation)
    }
}

/// Metadata and state specification of a command.
#[derive(Debug, Clone)]
pub struct CommandSpec {
    /// Unique command id.
    pub id: CommandId,
    /// Short localized label.
    pub label: String,
    /// Description for undo history.
    pub undo_label: String,
    /// Longer description for accessibility and tooltips.
    pub description: String,
    /// Whether command is currently enabled.
    pub enabled: bool,
    /// Whether command is checked (for toggle buttons and menu checkboxes).
    pub checked: bool,
    /// Radio group identifier, if this command belongs to a mutually exclusive group.
    pub radio_group: Option<String>,
    /// Whether this radio item is currently selected.
    pub selected: bool,
    /// Suggested default shortcut string.
    pub default_shortcut: Option<String>,
    /// Category for command palette grouping.
    pub category: String,
    /// Ordering index within its category.
    pub order: u32,
}

impl CommandSpec {
    /// Create a new command specification.
    pub fn new(id: impl Into<CommandId>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            undo_label: String::new(),
            description: String::new(),
            enabled: true,
            checked: false,
            radio_group: None,
            selected: false,
            default_shortcut: None,
            category: "general".into(),
            order: 100,
        }
    }

    /// Set undo label.
    pub fn with_undo_label(mut self, undo: impl Into<String>) -> Self {
        self.undo_label = undo.into();
        self
    }

    /// Set accessibility description.
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = desc.into();
        self
    }

    /// Set category.
    pub fn with_category(mut self, category: impl Into<String>) -> Self {
        self.category = category.into();
        self
    }

    /// Set default shortcut.
    pub fn with_shortcut(mut self, shortcut: impl Into<String>) -> Self {
        self.default_shortcut = Some(shortcut.into());
        self
    }

    /// Set initial enabled state.
    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Set radio group.
    pub fn with_radio_group(mut self, group: impl Into<String>) -> Self {
        self.radio_group = Some(group.into());
        self
    }

    /// Set ordering index within its category.
    pub fn with_order(mut self, order: u32) -> Self {
        self.order = order;
        self
    }
}

/// Authoritative command registry managing specs, state, search, and execution.
#[derive(Default)]
pub struct CommandRegistry {
    specs: HashMap<CommandId, CommandSpec>,
    handlers: HashMap<CommandId, Arc<dyn CommandHandler>>,
}

impl CommandRegistry {
    /// Create a new empty command registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a command specification without an immediate handler.
    pub fn register(&mut self, spec: CommandSpec) {
        self.specs.insert(spec.id.clone(), spec);
    }

    /// Register a command specification along with its handler.
    pub fn register_handler(&mut self, spec: CommandSpec, handler: impl CommandHandler + 'static) {
        let id = spec.id.clone();
        self.specs.insert(id.clone(), spec);
        self.handlers.insert(id, Arc::new(handler));
    }

    /// Register a closure handler for an existing or new command.
    pub fn register_fn<F>(&mut self, spec: CommandSpec, func: F)
    where
        F: Fn(&CommandInvocation) -> Result<CommandOutcome, CommandError> + Send + Sync + 'static,
    {
        self.register_handler(spec, FnCommandHandler::new(func));
    }

    /// Attach or replace the execution handler for a registered command id.
    pub fn set_handler(
        &mut self,
        id: impl Into<CommandId>,
        handler: impl CommandHandler + 'static,
    ) {
        self.handlers.insert(id.into(), Arc::new(handler));
    }

    /// Look up a command specification by id.
    pub fn get(&self, id: &CommandId) -> Option<&CommandSpec> {
        self.specs.get(id)
    }

    /// Mutably look up a command specification by id.
    pub fn get_mut(&mut self, id: &CommandId) -> Option<&mut CommandSpec> {
        self.specs.get_mut(id)
    }

    /// Update the enabled state of a command.
    pub fn set_enabled(&mut self, id: &CommandId, enabled: bool) -> bool {
        if let Some(spec) = self.specs.get_mut(id) {
            spec.enabled = enabled;
            true
        } else {
            false
        }
    }

    /// Update the checked state of a command.
    pub fn set_checked(&mut self, id: &CommandId, checked: bool) -> bool {
        if let Some(spec) = self.specs.get_mut(id) {
            spec.checked = checked;
            true
        } else {
            false
        }
    }

    /// Update radio group selection. Automatically unselects other items in the same radio group.
    pub fn select_radio(&mut self, id: &CommandId) -> bool {
        let group = match self.specs.get(id) {
            Some(spec) if spec.radio_group.is_some() => spec.radio_group.clone(),
            _ => return false,
        };
        if let Some(group_name) = group {
            for spec in self.specs.values_mut() {
                if spec.radio_group.as_deref() == Some(&group_name) {
                    spec.selected = spec.id == *id;
                }
            }
            true
        } else {
            false
        }
    }

    /// Authoritative invocation point. Enforces enablement and dispatches to handler.
    /// A command marked disabled will NEVER execute its handler.
    pub fn invoke(&self, invocation: &CommandInvocation) -> Result<CommandOutcome, CommandError> {
        let spec = self
            .specs
            .get(&invocation.id)
            .ok_or_else(|| CommandError::NotFound(invocation.id.clone()))?;

        if !spec.enabled {
            return Err(CommandError::Disabled(invocation.id.clone()));
        }

        let handler = self
            .handlers
            .get(&invocation.id)
            .ok_or_else(|| CommandError::NotFound(invocation.id.clone()))?;

        handler.execute(invocation)
    }

    /// Iterate over all registered command specifications.
    pub fn commands(&self) -> impl Iterator<Item = &CommandSpec> {
        self.specs.values()
    }

    /// Number of registered commands.
    pub fn len(&self) -> usize {
        self.specs.len()
    }

    /// Whether the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.specs.is_empty()
    }

    /// Search commands with deterministic scoring for palette filtering.
    pub fn search(&self, query: &str) -> Vec<(&CommandSpec, u32)> {
        let q = query.trim().to_lowercase();
        if q.is_empty() {
            return Vec::new();
        }
        let mut results: Vec<(&CommandSpec, u32)> = self
            .specs
            .values()
            .filter_map(|spec| {
                let mut score = 0u32;
                let label_lower = spec.label.to_lowercase();
                let id_lower = spec.id.0.to_lowercase();
                let category_lower = spec.category.to_lowercase();

                if label_lower == q {
                    score += 100;
                } else if label_lower.starts_with(&q) {
                    score += 50;
                } else if label_lower.contains(&q) {
                    score += 30;
                }

                if id_lower == q {
                    score += 40;
                } else if id_lower.contains(&q) {
                    score += 20;
                }

                if category_lower.contains(&q) {
                    score += 10;
                }

                if score > 0 {
                    Some((spec, score))
                } else {
                    None
                }
            })
            .collect();

        // Deterministic sort: highest score first, then category order, then alphabetic ID
        results.sort_by(|a, b| {
            b.1.cmp(&a.1)
                .then(a.0.order.cmp(&b.0.order))
                .then(a.0.id.cmp(&b.0.id))
        });
        results
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn sample_registry() -> CommandRegistry {
        let mut r = CommandRegistry::new();
        r.register_fn(CommandSpec::new("file.save", "Save"), |inv| {
            Ok(CommandOutcome::success(inv.id.clone()).with_message("saved"))
        });
        r.register_fn(CommandSpec::new("edit.undo", "Undo"), |inv| {
            Ok(CommandOutcome::success(inv.id.clone()).with_announcement("undone"))
        });
        r.register_fn(CommandSpec::new("edit.redo", "Redo"), |inv| {
            Ok(CommandOutcome::success(inv.id.clone()))
        });
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
    fn authoritative_invoke_success() {
        let r = sample_registry();
        let inv = CommandInvocation::new("file.save", InvocationSource::Menu);
        let outcome = r.invoke(&inv).expect("save should succeed");
        assert_eq!(outcome.id.as_str(), "file.save");
        assert_eq!(outcome.message.as_deref(), Some("saved"));
    }

    #[test]
    fn disabled_command_never_executes_handler() {
        let execution_count = Arc::new(AtomicUsize::new(0));
        let count_clone = execution_count.clone();

        let mut r = CommandRegistry::new();
        let mut spec = CommandSpec::new("test.action", "Action");
        spec.enabled = false;

        r.register_fn(spec, move |inv| {
            count_clone.fetch_add(1, Ordering::SeqCst);
            Ok(CommandOutcome::success(inv.id.clone()))
        });

        let inv = CommandInvocation::new("test.action", InvocationSource::Shortcut);
        let result = r.invoke(&inv);

        assert!(matches!(result, Err(CommandError::Disabled(_))));
        assert_eq!(execution_count.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn invoke_unregistered_fails_with_not_found() {
        let r = sample_registry();
        let inv = CommandInvocation::new("unknown.action", InvocationSource::Palette);
        assert!(matches!(r.invoke(&inv), Err(CommandError::NotFound(_))));
    }

    #[test]
    fn all_surfaces_invoke_same_handler() {
        let counter = Arc::new(AtomicUsize::new(0));
        let c = counter.clone();

        let mut r = CommandRegistry::new();
        r.register_fn(CommandSpec::new("action.tick", "Tick"), move |inv| {
            c.fetch_add(1, Ordering::SeqCst);
            Ok(CommandOutcome::success(inv.id.clone()))
        });

        for source in [
            InvocationSource::Toolbar,
            InvocationSource::Menu,
            InvocationSource::Shortcut,
            InvocationSource::Palette,
            InvocationSource::ContextMenu,
            InvocationSource::Accessibility,
            InvocationSource::Plugin,
            InvocationSource::Test,
        ] {
            let inv = CommandInvocation::new("action.tick", source);
            r.invoke(&inv).expect("invocation must succeed");
        }

        assert_eq!(counter.load(Ordering::SeqCst), 8);
    }

    #[test]
    fn search_deterministic_ranking() {
        let mut r = CommandRegistry::new();
        r.register(CommandSpec::new("file.save", "Save File"));
        r.register(CommandSpec::new("file.save_as", "Save As"));
        r.register(CommandSpec::new("edit.savearea", "Format Area"));

        let res = r.search("save");
        assert_eq!(res.len(), 3);
        assert_eq!(res[0].0.id.as_str(), "file.save");
        assert_eq!(res[1].0.id.as_str(), "file.save_as");
    }

    #[test]
    fn empty_query_returns_nothing() {
        let r = sample_registry();
        assert!(r.search("").is_empty());
        assert!(r.search("   ").is_empty());
    }

    #[test]
    fn radio_group_mutual_exclusion() {
        let mut r = CommandRegistry::new();
        r.register(CommandSpec::new("align.left", "Left").with_radio_group("alignment"));
        r.register(CommandSpec::new("align.center", "Center").with_radio_group("alignment"));
        r.register(CommandSpec::new("align.right", "Right").with_radio_group("alignment"));

        assert!(r.select_radio(&CommandId::new("align.center")));
        assert!(!r.get(&CommandId::new("align.left")).unwrap().selected);
        assert!(r.get(&CommandId::new("align.center")).unwrap().selected);
        assert!(!r.get(&CommandId::new("align.right")).unwrap().selected);

        assert!(r.select_radio(&CommandId::new("align.right")));
        assert!(!r.get(&CommandId::new("align.center")).unwrap().selected);
        assert!(r.get(&CommandId::new("align.right")).unwrap().selected);
    }
}
