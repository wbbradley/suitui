mod app;
mod coin_fetcher;
mod config;
mod ui;

use std::{io, path::PathBuf, time::Duration};

use anyhow::Result;
use app::AppAction;
use clap::Parser;
use crossterm::{
    ExecutableCommand,
    event::{self, Event},
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};

/// Sui wallet TUI
#[derive(Parser)]
struct Args {
    /// Path to the Sui client config file
    #[arg(long)]
    config: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let config_path = match args.config {
        Some(path) => path,
        None => config::default_config_path()?,
    };
    let wallet_data = config::load_wallet_data(&config_path)?;
    let mut app = app::App::new(wallet_data);

    let mut terminal = setup_terminal()?;
    let result = run_event_loop(&mut terminal, &mut app);
    restore_terminal(&mut terminal)?;
    result
}

fn setup_terminal() -> Result<Terminal<CrosstermBackend<io::Stdout>>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    stdout.execute(EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let terminal = Terminal::new(backend)?;
    Ok(terminal)
}

fn restore_terminal(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
    disable_raw_mode()?;
    terminal.backend_mut().execute(LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}

fn run_event_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut app::App,
) -> Result<()> {
    app.maybe_trigger_coin_fetch();
    app.maybe_trigger_chain_id_fetch();

    loop {
        terminal.draw(|frame| ui::draw(frame, app))?;

        if event::poll(Duration::from_millis(100))?
            && let Event::Key(key) = event::read()?
        {
            match app.handle_key(key) {
                AppAction::Quit => break,
                AppAction::Redraw | AppAction::None => {}
            }
        }

        while let Ok(result) = app.coin_rx.try_recv() {
            app.handle_coin_result(result);
        }
        while let Ok(result) = app.chain_id_rx.try_recv() {
            app.handle_chain_id_result(result);
        }

        app.maybe_trigger_coin_fetch();
        app.maybe_trigger_chain_id_fetch();

        if app.should_quit {
            break;
        }
    }
    Ok(())
}
