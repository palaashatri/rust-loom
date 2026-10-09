//! Present's command palette: which commands match a query, best match first. The
//! rows shown and the command that runs when a row is picked both come from here,
//! so the list and the choice always agree.

use loom_command::match_score;

use crate::PaletteCommand;

/// The commands that match `query`, best match first. An empty query keeps every
/// command in the palette's own order, and equal scores keep that order too.
pub(crate) fn matching(commands: Vec<PaletteCommand>, query: &str) -> Vec<PaletteCommand> {
    if query.trim().is_empty() {
        return commands;
    }
    let mut scored: Vec<(u32, PaletteCommand)> = commands
        .into_iter()
        .filter_map(|command| {
            let score = match_score(query, command.label, command.id, "");
            (score > 0).then_some((score, command))
        })
        .collect();
    // A stable sort keeps the palette's own order among equal scores.
    scored.sort_by_key(|(score, _)| std::cmp::Reverse(*score));
    scored.into_iter().map(|(_, command)| command).collect()
}
