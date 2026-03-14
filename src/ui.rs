use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Row, Table},
};
use sui_types::base_types::SuiAddress;

use crate::{
    app::{App, CoinState, Focus},
    coin_fetcher::{format_balance, short_coin_type},
};

pub fn draw(frame: &mut Frame, app: &mut App) {
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
}

fn border_style(focus: Focus, pane: Focus) -> Style {
    if focus == pane {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    }
}

fn draw_accounts(frame: &mut Frame, app: &mut App, area: Rect) {
    let env_label = app.pending_env.as_deref().unwrap_or("none");
    let title = format!("Accounts  [Env: {}]", env_label);
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(border_style(app.focus, Focus::Accounts));

    let items: Vec<ListItem> = app
        .accounts
        .iter()
        .map(|(addr, alias)| {
            let marker = if app.active_address == Some(*addr) {
                "* "
            } else {
                "  "
            };
            let line = format!("  {}{}  {}", marker, alias, addr);
            ListItem::new(line)
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

    frame.render_stateful_widget(list, area, &mut app.account_list_state);
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

fn alias_for(app: &App, addr: Option<SuiAddress>) -> Option<&str> {
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

    let has_pending = app.has_pending_changes();
    let dim = Style::default().fg(Color::DarkGray);
    let label_style = if has_pending {
        dim
    } else {
        Style::default().fg(Color::Gray)
    };
    let value_style = if has_pending { dim } else { Style::default() };

    let mut lines = Vec::new();

    // ── Active ──
    let active_header_style = if has_pending {
        dim
    } else {
        Style::default().fg(Color::Gray)
    };
    lines.push(Line::from(Span::styled(
        "── Active ──",
        active_header_style,
    )));

    if let Some(env) = app.active_env_info() {
        lines.push(Line::from(vec![
            Span::styled("Env:     ", label_style),
            Span::styled(env.alias.clone(), value_style),
        ]));
        if !has_pending {
            lines.push(Line::from(vec![
                Span::styled("RPC:     ", label_style),
                Span::styled(env.rpc.clone(), value_style),
            ]));
            lines.push(Line::from(vec![
                Span::styled("Chain:   ", label_style),
                Span::styled(
                    env.chain_id.as_deref().unwrap_or("unknown").to_string(),
                    value_style,
                ),
            ]));
        }
    } else {
        lines.push(Line::from(Span::styled("No active environment", dim)));
    }

    let active_alias = alias_for(app, app.active_address).unwrap_or("none");
    lines.push(Line::from(vec![
        Span::styled("Account: ", label_style),
        Span::styled(active_alias, value_style),
    ]));

    // ── Pending ──
    if has_pending {
        lines.push(Line::from(""));
        let pending_header = Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD);
        lines.push(Line::from(Span::styled(
            "── Pending (F10 to apply) ──",
            pending_header,
        )));

        let plabel = Style::default().fg(Color::Gray);
        if let Some(env) = app.pending_env_info() {
            lines.push(Line::from(vec![
                Span::styled("Env:     ", plabel),
                Span::raw(env.alias.clone()),
            ]));
            lines.push(Line::from(vec![
                Span::styled("RPC:     ", plabel),
                Span::raw(env.rpc.clone()),
            ]));
            lines.push(Line::from(vec![
                Span::styled("Chain:   ", plabel),
                Span::raw(env.chain_id.as_deref().unwrap_or("unknown").to_string()),
            ]));
        }
        let pending_alias = alias_for(app, app.selected_account_address()).unwrap_or("none");
        lines.push(Line::from(vec![
            Span::styled("Account: ", plabel),
            Span::raw(pending_alias),
        ]));
    }

    let paragraph = Paragraph::new(lines).block(block);
    frame.render_widget(paragraph, area);
}

fn draw_help_bar(frame: &mut Frame, app: &mut App, area: Rect) {
    let (f10_key_style, f10_desc_style) = if app.has_pending_changes() {
        (Style::default().fg(Color::Cyan), Style::default())
    } else {
        (
            Style::default().fg(Color::DarkGray),
            Style::default().fg(Color::DarkGray),
        )
    };
    let help = Paragraph::new(Line::from(vec![
        Span::styled("F10", f10_key_style),
        Span::styled(": Apply  ", f10_desc_style),
        Span::styled("q", Style::default().fg(Color::Cyan)),
        Span::raw(": Quit  "),
        Span::styled("↑↓", Style::default().fg(Color::Cyan)),
        Span::raw(": Navigate  "),
        Span::styled("Tab", Style::default().fg(Color::Cyan)),
        Span::raw(": Switch pane  "),
        Span::styled("e", Style::default().fg(Color::Cyan)),
        Span::raw(": Env"),
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
