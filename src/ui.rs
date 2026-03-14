use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Row, Table},
};
use sui_sdk_types::Address;

use crate::{
    app::{App, CoinState, Focus, ObjectState, View},
    coin_fetcher::{format_balance, short_coin_type},
};

pub fn draw(frame: &mut Frame, app: &mut App) {
    match app.current_view() {
        View::Main => draw_main(frame, app),
        View::ObjectInspector(addr) => draw_object_inspector(frame, app, addr),
    }
}

fn draw_main(frame: &mut Frame, app: &mut App) {
    let outer = Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).split(frame.area());

    let main_area = outer[0];
    let help_area = outer[1];

    let main_cols = Layout::horizontal([Constraint::Percentage(60), Constraint::Percentage(40)])
        .split(main_area);

    let left_rows = Layout::vertical([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(main_cols[0]);

    draw_accounts(frame, app, left_rows[0]);
    draw_coins(frame, app, left_rows[1]);
    draw_network_info(frame, app, main_cols[1]);
    draw_help_bar(frame, app, help_area);

    if app.env_dropdown_open {
        draw_env_dropdown(frame, app, left_rows[0]);
    }
    if app.address_input_open {
        draw_address_input(frame, app, frame.area());
    }
}

fn border_style(focus: Focus, pane: Focus) -> Style {
    if focus == pane {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    }
}

fn short_address(addr: &Address) -> String {
    let s = addr.to_string();
    if s.len() > 12 {
        format!("{}...{}", &s[..6], &s[s.len() - 4..])
    } else {
        s
    }
}

fn draw_accounts(frame: &mut Frame, app: &mut App, area: Rect) {
    let env_label = app.active_env.as_deref().unwrap_or("none");
    let title = format!("Accounts  [Env: {}]", env_label);
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(border_style(app.focus, Focus::Accounts));

    let rows: Vec<Row> = app
        .accounts
        .iter()
        .map(|(addr, alias)| {
            let marker = if app.active_address == Some(*addr) {
                "* "
            } else {
                "  "
            };
            Row::new(vec![marker.to_string(), alias.clone(), short_address(addr)])
        })
        .collect();

    let max_alias = app
        .accounts
        .iter()
        .map(|(_, alias)| alias.len() as u16)
        .max()
        .unwrap_or(12)
        .max(5);

    let widths = [
        Constraint::Length(2),
        Constraint::Length(max_alias),
        Constraint::Min(0),
    ];
    let table = Table::new(rows, widths)
        .block(block)
        .row_highlight_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("> ");

    frame.render_stateful_widget(table, area, &mut app.account_list_state);
}

fn draw_coins(frame: &mut Frame, app: &mut App, area: Rect) {
    let addr_label = app
        .selected_account_address()
        .and_then(|a| {
            app.accounts
                .iter()
                .find(|(addr, _)| *addr == a)
                .map(|(_, alias)| alias.as_str())
        })
        .unwrap_or("none");

    let block = Block::default()
        .title(format!("Coins for {}", addr_label))
        .borders(Borders::ALL)
        .border_style(border_style(app.focus, Focus::Coins));

    match &app.coin_state {
        CoinState::Idle => {
            let p = Paragraph::new("  Select an account and environment")
                .style(Style::default().fg(Color::DarkGray))
                .block(block);
            frame.render_widget(p, area);
        }
        CoinState::Loading => {
            let p = Paragraph::new("  Loading balances...")
                .style(Style::default().fg(Color::Yellow))
                .block(block);
            frame.render_widget(p, area);
        }
        CoinState::Error(msg) => {
            let p = Paragraph::new(format!("  Error: {msg}"))
                .style(Style::default().fg(Color::Red))
                .block(block);
            frame.render_widget(p, area);
        }
        CoinState::Loaded(balances) => {
            if balances.is_empty() {
                let p = Paragraph::new("  No coins found")
                    .style(Style::default().fg(Color::DarkGray))
                    .block(block);
                frame.render_widget(p, area);
            } else {
                let rows: Vec<Row> = balances
                    .iter()
                    .map(|b| {
                        Row::new(vec![
                            short_coin_type(&b.coin_type).to_string(),
                            format_balance(b.total_balance, 9),
                        ])
                    })
                    .collect();
                let widths = [Constraint::Percentage(40), Constraint::Percentage(60)];
                let table = Table::new(rows, widths)
                    .header(
                        Row::new(vec!["Coin", "Balance"])
                            .style(Style::default().add_modifier(Modifier::BOLD)),
                    )
                    .block(block);
                frame.render_widget(table, area);
            }
        }
    }
}

fn alias_for(app: &App, addr: Option<Address>) -> Option<&str> {
    let addr = addr?;
    app.accounts
        .iter()
        .find(|(a, _)| *a == addr)
        .map(|(_, alias)| alias.as_str())
}

fn draw_network_info(frame: &mut Frame, app: &mut App, area: Rect) {
    let block = Block::default()
        .title("Network Info")
        .borders(Borders::ALL)
        .border_style(border_style(app.focus, Focus::NetworkInfo));

    let label_style = Style::default().fg(Color::Gray);

    let mut lines = Vec::new();

    if let Some(env) = app.active_env_info() {
        lines.push(Line::from(vec![
            Span::styled("Env:     ", label_style),
            Span::raw(env.alias.clone()),
        ]));
        lines.push(Line::from(vec![
            Span::styled("RPC:     ", label_style),
            Span::raw(env.rpc.clone()),
        ]));
        let chain_display = env
            .chain_id
            .as_deref()
            .or_else(|| app.chain_id_cache.get(&env.rpc).map(|s| s.as_str()))
            .unwrap_or(if app.chain_id_fetch_pending.as_ref() == Some(&env.rpc) {
                "fetching..."
            } else {
                "unknown"
            });
        lines.push(Line::from(vec![
            Span::styled("Chain:   ", label_style),
            Span::raw(chain_display.to_string()),
        ]));
    } else {
        lines.push(Line::from(Span::styled(
            "No active environment",
            Style::default().fg(Color::DarkGray),
        )));
    }

    let active_alias = alias_for(app, app.active_address).unwrap_or("none");
    lines.push(Line::from(vec![
        Span::styled("Account: ", label_style),
        Span::raw(active_alias),
    ]));

    let paragraph = Paragraph::new(lines).block(block);
    frame.render_widget(paragraph, area);
}

fn draw_help_bar(frame: &mut Frame, _app: &mut App, area: Rect) {
    let help = Paragraph::new(Line::from(vec![
        Span::styled("Enter", Style::default().fg(Color::Cyan)),
        Span::raw(": Select  "),
        Span::styled("q", Style::default().fg(Color::Cyan)),
        Span::raw(": Quit  "),
        Span::styled("↑↓", Style::default().fg(Color::Cyan)),
        Span::raw(": Navigate  "),
        Span::styled("Tab", Style::default().fg(Color::Cyan)),
        Span::raw(": Switch pane  "),
        Span::styled("e", Style::default().fg(Color::Cyan)),
        Span::raw(": Env  "),
        Span::styled("i", Style::default().fg(Color::Cyan)),
        Span::raw(": Inspect  "),
        Span::styled("r", Style::default().fg(Color::Cyan)),
        Span::raw(": Refresh"),
    ]));
    frame.render_widget(help, area);
}

fn draw_env_dropdown(frame: &mut Frame, app: &mut App, anchor: Rect) {
    let width = 30u16.min(anchor.width);
    let height = (app.envs.len() as u16 + 2).min(anchor.height);
    let x = anchor.right().saturating_sub(width);
    let y = anchor.y + 1;
    let dropdown_area = Rect::new(x, y, width, height);

    frame.render_widget(Clear, dropdown_area);

    let block = Block::default()
        .title("Select Env")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    let items: Vec<ListItem> = app
        .envs
        .iter()
        .map(|e| {
            let marker = if app.active_env.as_ref() == Some(&e.alias) {
                " *"
            } else {
                ""
            };
            ListItem::new(format!("  {}{}", e.alias, marker))
        })
        .collect();

    let list = List::new(items)
        .block(block)
        .highlight_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("> ");

    frame.render_stateful_widget(list, dropdown_area, &mut app.env_list_state);
}

fn draw_address_input(frame: &mut Frame, app: &App, area: Rect) {
    let width = 60u16.min(area.width.saturating_sub(4));
    let height = 6u16;
    let x = (area.width.saturating_sub(width)) / 2 + area.x;
    let y = (area.height.saturating_sub(height)) / 2 + area.y;
    let modal_area = Rect::new(x, y, width, height);

    frame.render_widget(Clear, modal_area);

    let block = Block::default()
        .title("Inspect Object")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    let inner = block.inner(modal_area);
    frame.render_widget(block, modal_area);

    let input_line = format!("{}█", &app.address_input);
    let input = Paragraph::new(input_line);
    let input_area = Rect::new(inner.x, inner.y, inner.width, 1);
    frame.render_widget(input, input_area);

    if let Some(err) = &app.address_input_error {
        let err_line = Paragraph::new(err.as_str()).style(Style::default().fg(Color::Red));
        let err_area = Rect::new(inner.x, inner.y + 1, inner.width, 1);
        frame.render_widget(err_line, err_area);
    }

    let help =
        Paragraph::new("Enter: Inspect  Esc: Cancel").style(Style::default().fg(Color::DarkGray));
    let help_area = Rect::new(
        inner.x,
        inner.y + inner.height.saturating_sub(1),
        inner.width,
        1,
    );
    frame.render_widget(help, help_area);
}

fn draw_object_inspector(frame: &mut Frame, app: &App, addr: Address) {
    let block = Block::default()
        .title(format!("Object Inspector: {}", addr))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    let text = match &app.object_state {
        ObjectState::Idle => "  Waiting...".to_string(),
        ObjectState::Loading => "  Loading object...".to_string(),
        ObjectState::Error(msg) => format!("  Error: {msg}"),
        ObjectState::Loaded(data) => format!(
            "  Type: {}\n  Version: {}\n  Digest: {}\n\n  Press Esc or q to go back",
            data.object_type, data.version, data.digest
        ),
    };
    let style = match &app.object_state {
        ObjectState::Error(_) => Style::default().fg(Color::Red),
        _ => Style::default().fg(Color::DarkGray),
    };
    let p = Paragraph::new(text).style(style).block(block);
    frame.render_widget(p, frame.area());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_address_truncates() {
        let addr = Address::from_bytes([7u8; 32]).unwrap();
        let full = addr.to_string();
        let short = short_address(&addr);
        assert_eq!(short.len(), 13); // "0xABCD...EF12"
        assert!(short.starts_with(&full[..6]));
        assert!(short.ends_with(&full[full.len() - 4..]));
        assert!(short.contains("..."));
    }
}
