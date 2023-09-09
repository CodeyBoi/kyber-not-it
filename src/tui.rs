use crossterm::{
    cursor,
    event::{self, DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{
        self, disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
    },
};
use ratatui::{
    prelude::CrosstermBackend,
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, List, ListItem, ListState},
    Terminal,
};

use std::{io, thread, time::Duration};

pub(crate) fn main() -> io::Result<()> {
    enable_raw_mode()?;

    let mut stdout = io::stdout();
    execute!(
        stdout,
        EnterAlternateScreen,
        EnableMouseCapture,
        cursor::Hide,
        terminal::Clear(terminal::ClearType::All)
    )?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    terminal.draw(|f| {
        let mut size = f.size();
        let block = Block::default().title("KyberKracker").borders(Borders::ALL);
        f.render_widget(block, size);
        let items = ["Profile", "Attack", "Exit"];
        let (list, mut state) = menu(&items);
        size.height = size.height - 2;
        size.width = size.width - 2;
        size.x = 1;
        size.y = 1;
        f.render_stateful_widget(list, size, &mut state);
    })?;

    thread::spawn(|| loop {
        event::read().unwrap();
    });

    thread::sleep(Duration::from_millis(5000));

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture,
        cursor::Show
    )?;

    Ok(())
}

fn menu<'a>(items: &'a [&'a str]) -> (List<'a>, ListState) {
    let items: Vec<_> = items.iter().map(|s| ListItem::new(s.to_string())).collect();
    let list = List::new(items)
        .block(Block::default().title("Commands").borders(Borders::ALL))
        .style(Style::default().fg(Color::White))
        .highlight_style(Style::default().add_modifier(Modifier::ITALIC))
        .highlight_symbol(">>");
    let state = ListState::default();
    (list, state)
}
