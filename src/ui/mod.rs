pub mod action;
pub mod input;
pub mod keymap;
pub mod render;
pub mod state;
pub mod theme;
pub mod update;

use std::io::{self, Stdout};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crossterm::event::{
    Event, EventStream, KeyEventKind, KeyboardEnhancementFlags,
    PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode,
    enable_raw_mode, supports_keyboard_enhancement,
};
use futures::StreamExt;
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

use crate::agent::AgentRuntime;
use crate::engine::TiroEngine;
use crate::filter::Filter;
use crate::store::NoteStore;

use keymap::Keymap;
use state::{AppState, SearchState};

const TICK: Duration = Duration::from_millis(100);

pub async fn run<S: NoteStore + Send + 'static>(
    engine: Arc<Mutex<TiroEngine<S>>>,
    runtime: AgentRuntime,
) -> io::Result<()> {
    let (mut terminal, enhanced_kbd) = setup_terminal()?;
    let result = main_loop(&mut terminal, engine, runtime).await;
    restore_terminal(&mut terminal, enhanced_kbd)?;
    result
}

fn setup_terminal() -> io::Result<(Terminal<CrosstermBackend<Stdout>>, bool)> {
    let mut stdout = io::stdout();
    enable_raw_mode()?;
    execute!(stdout, EnterAlternateScreen)?;
    // Ask supporting terminals (kitty, foot, wezterm, ghostty, alacritty
    // >=0.13, recent iTerm2) to disambiguate keys like Ctrl+/ vs Ctrl+_
    // and report shifted variants. On unsupported terminals these keys
    // remain ambiguous, but legacy bindings still work.
    let enhanced_kbd = supports_keyboard_enhancement().unwrap_or(false);
    if enhanced_kbd {
        execute!(
            stdout,
            PushKeyboardEnhancementFlags(
                KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
                    | KeyboardEnhancementFlags::REPORT_ALTERNATE_KEYS,
            )
        )?;
    }
    let terminal = Terminal::new(CrosstermBackend::new(stdout))?;
    Ok((terminal, enhanced_kbd))
}

fn restore_terminal(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    enhanced_kbd: bool,
) -> io::Result<()> {
    if enhanced_kbd {
        execute!(terminal.backend_mut(), PopKeyboardEnhancementFlags)?;
    }
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}

async fn main_loop<S: NoteStore + Send + 'static>(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    engine: Arc<Mutex<TiroEngine<S>>>,
    runtime: AgentRuntime,
) -> io::Result<()> {
    let keymap = Keymap::default();

    let mut search = SearchState::new();
    {
        let filter = Filter::parse(&search.query.text());
        if let Ok(rows) = engine
            .lock()
            .expect("engine mutex")
            .get_notes_page(&filter, 0)
        {
            search.results = rows;
        }
    }
    let mut state = AppState::new(search);

    let mut events = EventStream::new();
    let mut tick_interval = tokio::time::interval(TICK);

    while !state.quit {
        terminal.draw(|f| render::dispatch(&state, &engine, f))?;

        tokio::select! {
            biased;
            maybe_ev = events.next() => {
                match maybe_ev {
                    Some(Ok(Event::Key(key))) => {
                        if key.kind != KeyEventKind::Press {
                            continue;
                        }
                        let _dismissed_error = state.error.take().is_some();
                        if let Some(action) = keymap.translate(key, &state) {
                            update::apply(&mut state, action, &engine, &runtime);
                        }
                    }
                    Some(Ok(Event::Resize(_, _))) => {}
                    Some(Ok(_)) => {}
                    Some(Err(e)) => return Err(e),
                    None => break,
                }
            }
            _ = tick_interval.tick() => {
                update::tick(&mut state, &engine, &runtime);
            }
        }
    }
    Ok(())
}
