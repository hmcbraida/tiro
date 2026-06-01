pub mod action;
pub mod input;
pub mod keymap;
pub mod render;
pub mod state;
pub mod theme;
pub mod update;

use std::io::{self, Stdout};
use std::time::Duration;

use crossterm::event::{self, Event};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode,
    enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

use crate::engine::TiroEngine;
use crate::store::NoteStore;

use state::{AppState, SearchState};

const TICK: Duration = Duration::from_millis(100);

/// Initialise the application UI.
///
/// Takes control of the terminal, enters raw mode, and inits the relevant
/// UI objects.
pub fn run<S: NoteStore>(mut engine: TiroEngine<S>) -> io::Result<()> {
    let mut terminal = setup_terminal()?;
    let result = main_loop(&mut terminal, &mut engine);
    // application exit: give user their terminal back
    restore_terminal(&mut terminal)?;
    result
}

fn setup_terminal() -> io::Result<Terminal<CrosstermBackend<Stdout>>> {
    let mut stdout = io::stdout();
    enable_raw_mode()?;
    execute!(stdout, EnterAlternateScreen)?;
    Terminal::new(CrosstermBackend::new(stdout))
}

fn restore_terminal(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
) -> io::Result<()> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}

fn main_loop<S: NoteStore>(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    engine: &mut TiroEngine<S>,
) -> io::Result<()> {
    let mut search = SearchState::new();
    if let Ok(rows) = engine.get_notes_page(&String::new(), 0) {
        search.results = rows;
    }
    let mut state = AppState::new(search);

    while !state.quit {
        terminal.draw(|f| render::dispatch(&state, engine, f))?;

        if event::poll(TICK)? {
            match event::read()? {
                Event::Key(key) => {
                    use crossterm::event::KeyEventKind;
                    if key.kind != KeyEventKind::Press {
                        continue;
                    }
                    let dismissed_error = state.error.take().is_some();
                    if let Some(action) = keymap::translate(key, &state) {
                        update::apply(&mut state, action, engine);
                    }
                    let _ = dismissed_error;
                }
                Event::Resize(_, _) => {}
                _ => {}
            }
        } else {
            update::tick(&mut state, engine);
        }
    }
    Ok(())
}
