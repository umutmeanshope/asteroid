use asteroid_core::parse;
use asteroid_core::{Backlink, Heading, NoteSummary, SearchHit, TagCount, render};
use tauri::State;

use crate::state::{AppState, CmdResult};

#[tauri::command(async)]
pub fn note_index(state: State<'_, AppState>) -> CmdResult<Vec<NoteSummary>> {
    state.with(|open| open.index.summaries())
}

#[tauri::command(async)]
pub fn search(state: State<'_, AppState>, query: String) -> CmdResult<Vec<SearchHit>> {
    state.with(|open| open.index.search(&query, 200))
}

#[tauri::command(async)]
pub fn backlinks(state: State<'_, AppState>, path: String) -> CmdResult<Vec<Backlink>> {
    state.with(|open| open.index.backlinks(&path))
}

/// Headings of the given (possibly unsaved) note text.
#[tauri::command(async)]
pub fn outline(content: String) -> Vec<Heading> {
    parse::parse(&content).headings
}

#[tauri::command(async)]
pub fn resolve_link(state: State<'_, AppState>, target: String, from: String) -> CmdResult<Option<String>> {
    state.with(|open| open.index.resolve(&target, &from))
}

#[tauri::command(async)]
pub fn render_note(state: State<'_, AppState>, path: String, content: String) -> CmdResult<String> {
    state.with(|open| render::render(&content, &path, &open.index))
}

/// An embedded note (`Note`, `Note#Heading`, `Note#^block`) as HTML, for the
/// editor's live preview. None when the target isn't a note.
#[tauri::command(async)]
pub fn render_embed(state: State<'_, AppState>, target: String, from: String) -> CmdResult<Option<String>> {
    state.with(|open| render::render_embed(&target, &from, &open.index))
}

#[tauri::command(async)]
pub fn tags(state: State<'_, AppState>) -> CmdResult<Vec<TagCount>> {
    state.with(|open| open.index.tags())
}
