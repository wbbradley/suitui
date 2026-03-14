use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Row, Table},
};
use sui_sdk_types::Address;

use crate::{
    app::{App, CoinState, DynFieldsState, Focus, ObjectState, TxHistoryState, View},
    coin_fetcher::{format_balance, short_coin_type},
    object_fetcher::{DynFieldKind, ObjectData, OwnerInfo},
    transaction_fetcher::{self, TransactionSummary, TxBalanceChange},
};

pub fn draw(frame: &mut Frame, app: &mut App) {
    match app.current_view() {
        View::Main => draw_main(frame, app),
        View::ObjectInspector(addr) => draw_object_inspector(frame, app, addr),
        View::TransactionHistory(addr) => draw_transaction_history(frame, app, addr),
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
        Span::styled("t", Style::default().fg(Color::Cyan)),
        Span::raw(": Tx History  "),
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
    let outer = Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).split(frame.area());
    let content_area = outer[0];
    let help_area = outer[1];

    let block = Block::default()
        .title(format!("Object Inspector: {}", addr))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    match &app.object_state {
        ObjectState::Idle | ObjectState::Loading => {
            let msg = if matches!(app.object_state, ObjectState::Loading) {
                "Loading object..."
            } else {
                "Waiting..."
            };
            let p = Paragraph::new(format!("  {msg}"))
                .style(Style::default().fg(Color::Yellow))
                .block(block);
            frame.render_widget(p, content_area);
        }
        ObjectState::Error(msg) => {
            let p = Paragraph::new(format!("  Error: {msg}"))
                .style(Style::default().fg(Color::Red))
                .block(block);
            frame.render_widget(p, content_area);
        }
        ObjectState::Loaded(data) => {
            let selected = app.inspector_sel;
            let mut link_idx = 0usize;
            let mut lines = Vec::new();
            append_metadata_lines(&mut lines, data, selected, &mut link_idx);
            append_properties_lines(&mut lines, data);
            append_dyn_fields_lines(&mut lines, &app.dyn_fields_state, selected, &mut link_idx);

            let p = Paragraph::new(lines).block(block);
            frame.render_widget(p, content_area);
        }
    }

    draw_inspector_help_bar(frame, help_area);
}

fn format_owner(owner: &OwnerInfo) -> String {
    match owner {
        OwnerInfo::Address(a) => format!("{a} (Address)"),
        OwnerInfo::Object(a) => format!("{a} (Object)"),
        OwnerInfo::Shared => "Shared".into(),
        OwnerInfo::Immutable => "Immutable".into(),
        OwnerInfo::Unknown => "Unknown".into(),
    }
}

fn append_metadata_lines<'a>(
    lines: &mut Vec<Line<'a>>,
    data: &ObjectData,
    selected: usize,
    link_idx: &mut usize,
) {
    let label = Style::default().fg(Color::Gray);

    lines.push(Line::from(vec![
        Span::styled("  Type:     ", label),
        Span::raw(data.object_type.clone()),
    ]));
    lines.push(Line::from(vec![
        Span::styled("  Version:  ", label),
        Span::raw(data.version.to_string()),
    ]));
    lines.push(Line::from(vec![
        Span::styled("  Digest:   ", label),
        Span::raw(data.digest.clone()),
    ]));

    let owner_is_linkable = matches!(&data.owner, OwnerInfo::Address(a) | OwnerInfo::Object(a) if a.parse::<Address>().is_ok());
    if owner_is_linkable {
        let is_selected = *link_idx == selected;
        let prefix = if is_selected { "> " } else { "  " };
        let value_style = if is_selected {
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Cyan)
        };
        lines.push(Line::from(vec![
            Span::styled(format!("{prefix}Owner:    "), label),
            Span::styled(format_owner(&data.owner), value_style),
        ]));
        *link_idx += 1;
    } else {
        lines.push(Line::from(vec![
            Span::styled("  Owner:    ", label),
            Span::raw(format_owner(&data.owner)),
        ]));
    }

    lines.push(Line::from(vec![
        Span::styled("  Prev Tx:  ", label),
        Span::raw(data.previous_transaction.clone()),
    ]));
    lines.push(Line::from(vec![
        Span::styled("  Rebate:   ", label),
        Span::raw(data.storage_rebate.to_string()),
    ]));
    if let Some(bal) = data.balance {
        lines.push(Line::from(vec![
            Span::styled("  Balance:  ", label),
            Span::raw(bal.to_string()),
        ]));
    }
}

fn append_properties_lines<'a>(lines: &mut Vec<Line<'a>>, data: &ObjectData) {
    let Some(json) = &data.json else { return };
    lines.push(Line::raw(""));
    lines.push(Line::styled(
        "  ── Properties ──",
        Style::default().fg(Color::Cyan),
    ));
    render_json_lines(lines, json, 2);
}

fn render_json_lines(lines: &mut Vec<Line<'_>>, value: &serde_json::Value, indent: usize) {
    let prefix = " ".repeat(indent);
    let label_style = Style::default().fg(Color::Gray);

    match value {
        serde_json::Value::Object(map) => {
            for (k, v) in map {
                if v.is_object() || v.is_array() {
                    lines.push(Line::from(Span::styled(
                        format!("{prefix}{k}:"),
                        label_style,
                    )));
                    render_json_lines(lines, v, indent + 2);
                } else {
                    lines.push(Line::from(vec![
                        Span::styled(format!("{prefix}{k}: "), label_style),
                        Span::raw(format_json_scalar(v)),
                    ]));
                }
            }
        }
        serde_json::Value::Array(arr) => {
            for (i, v) in arr.iter().enumerate() {
                if v.is_object() || v.is_array() {
                    lines.push(Line::from(Span::styled(
                        format!("{prefix}[{i}]:"),
                        label_style,
                    )));
                    render_json_lines(lines, v, indent + 2);
                } else {
                    lines.push(Line::from(vec![
                        Span::styled(format!("{prefix}[{i}]: "), label_style),
                        Span::raw(format_json_scalar(v)),
                    ]));
                }
            }
        }
        other => {
            lines.push(Line::from(Span::raw(format!(
                "{prefix}{}",
                format_json_scalar(other)
            ))));
        }
    }
}

fn format_json_scalar(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => format!("\"{s}\""),
        serde_json::Value::Null => "null".into(),
        other => other.to_string(),
    }
}

fn append_dyn_fields_lines<'a>(
    lines: &mut Vec<Line<'a>>,
    state: &DynFieldsState,
    selected: usize,
    link_idx: &mut usize,
) {
    lines.push(Line::raw(""));
    match state {
        DynFieldsState::Idle => {}
        DynFieldsState::Loading => {
            lines.push(Line::styled(
                "  ── Dynamic Fields ──",
                Style::default().fg(Color::Cyan),
            ));
            lines.push(Line::styled(
                "  Loading...",
                Style::default().fg(Color::Yellow),
            ));
        }
        DynFieldsState::Error(msg) => {
            lines.push(Line::styled(
                "  ── Dynamic Fields ──",
                Style::default().fg(Color::Cyan),
            ));
            lines.push(Line::styled(
                format!("  Error: {msg}"),
                Style::default().fg(Color::Red),
            ));
        }
        DynFieldsState::Loaded(fields) => {
            lines.push(Line::styled(
                format!("  ── Dynamic Fields ({}) ──", fields.len()),
                Style::default().fg(Color::Cyan),
            ));
            if fields.is_empty() {
                lines.push(Line::styled(
                    "  (none)",
                    Style::default().fg(Color::DarkGray),
                ));
            } else {
                for f in fields {
                    let kind_str = match f.kind {
                        DynFieldKind::Field => "Field ",
                        DynFieldKind::Object => "Object",
                        DynFieldKind::Unknown => "???   ",
                    };
                    let is_linkable = f.child_id.is_some()
                        && f.child_id
                            .as_ref()
                            .is_some_and(|id| id.parse::<Address>().is_ok());
                    if is_linkable {
                        let is_selected = *link_idx == selected;
                        let prefix = if is_selected { "> " } else { "  " };
                        let child = f.child_id.clone().unwrap_or_default();
                        let child_style = if is_selected {
                            Style::default()
                                .fg(Color::Yellow)
                                .add_modifier(Modifier::BOLD)
                        } else {
                            Style::default().fg(Color::Cyan)
                        };
                        lines.push(Line::from(vec![
                            Span::raw(prefix.to_string()),
                            Span::styled(kind_str, Style::default().fg(Color::Yellow)),
                            Span::raw(format!("  {}  {}  ", f.field_id, f.value_type)),
                            Span::styled(child, child_style),
                        ]));
                        *link_idx += 1;
                    } else {
                        let child = f.child_id.as_deref().unwrap_or("");
                        lines.push(Line::from(vec![
                            Span::styled("  ", Style::default()),
                            Span::styled(kind_str, Style::default().fg(Color::Yellow)),
                            Span::raw(format!("  {}  {}  {}", f.field_id, f.value_type, child)),
                        ]));
                    }
                }
            }
        }
    }
}

fn draw_inspector_help_bar(frame: &mut Frame, area: Rect) {
    let help = Paragraph::new(Line::from(vec![
        Span::styled("Esc", Style::default().fg(Color::Cyan)),
        Span::raw(": Back  "),
        Span::styled("Enter", Style::default().fg(Color::Cyan)),
        Span::raw(": Inspect  "),
        Span::styled("↑↓", Style::default().fg(Color::Cyan)),
        Span::raw(": Navigate  "),
        Span::styled("r", Style::default().fg(Color::Cyan)),
        Span::raw(": Refresh"),
    ]));
    frame.render_widget(help, area);
}

fn draw_transaction_history(frame: &mut Frame, app: &mut App, addr: Address) {
    let outer = Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).split(frame.area());
    let content_area = outer[0];
    let help_area = outer[1];

    let alias = alias_for(app, Some(addr)).unwrap_or("unknown");
    let block = Block::default()
        .title(format!("Transaction History: {}", alias))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    // Extract state info before mutably borrowing app for table rendering
    enum TxDisplay {
        Message(String, Color),
        Table(Vec<TransactionSummary>),
    }
    let display = match &app.tx_history_state {
        TxHistoryState::Idle => TxDisplay::Message("Waiting...".into(), Color::Yellow),
        TxHistoryState::Loading => {
            TxDisplay::Message("Loading transactions...".into(), Color::Yellow)
        }
        TxHistoryState::Error(msg) => TxDisplay::Message(format!("Error: {msg}"), Color::Red),
        TxHistoryState::Loaded(txs) if txs.is_empty() => {
            TxDisplay::Message("No transactions found".into(), Color::DarkGray)
        }
        TxHistoryState::Loaded(txs) => TxDisplay::Table(txs.clone()),
    };
    match display {
        TxDisplay::Message(msg, color) => {
            let p = Paragraph::new(format!("  {msg}"))
                .style(Style::default().fg(color))
                .block(block);
            frame.render_widget(p, content_area);
        }
        TxDisplay::Table(txs) => {
            draw_tx_table(frame, app, &txs, block, content_area);
        }
    }

    draw_tx_history_help_bar(frame, help_area);
}

fn draw_tx_table(
    frame: &mut Frame,
    app: &mut App,
    txs: &[TransactionSummary],
    block: Block,
    area: Rect,
) {
    let rows: Vec<Row> = txs
        .iter()
        .map(|tx| {
            let digest = short_digest(&tx.digest);
            let time = tx
                .timestamp
                .as_ref()
                .map(transaction_fetcher::format_timestamp)
                .unwrap_or_else(|| "?".into());
            let status = match tx.success {
                Some(true) => "OK".to_string(),
                Some(false) => "FAIL".to_string(),
                None => "?".to_string(),
            };
            let gas = tx
                .gas_used
                .as_ref()
                .map(|g| {
                    let total = g.computation_cost.saturating_add(g.storage_cost);
                    let net = total.saturating_sub(g.storage_rebate);
                    format_balance(net, 9)
                })
                .unwrap_or_else(|| "?".into());
            let changes = format_balance_changes(&tx.balance_changes);
            Row::new(vec![digest, time, status, gas, changes])
        })
        .collect();

    let header = Row::new(vec!["Digest", "Time", "Status", "Gas", "Balance Changes"])
        .style(Style::default().add_modifier(Modifier::BOLD));

    let widths = [
        Constraint::Length(13),
        Constraint::Length(16),
        Constraint::Length(6),
        Constraint::Length(12),
        Constraint::Min(0),
    ];

    let table = Table::new(rows, widths)
        .header(header)
        .block(block)
        .row_highlight_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("> ");

    frame.render_stateful_widget(table, area, &mut app.tx_history_table_state);
}

fn draw_tx_history_help_bar(frame: &mut Frame, area: Rect) {
    let help = Paragraph::new(Line::from(vec![
        Span::styled("Esc", Style::default().fg(Color::Cyan)),
        Span::raw(": Back  "),
        Span::styled("↑↓", Style::default().fg(Color::Cyan)),
        Span::raw(": Navigate  "),
        Span::styled("r", Style::default().fg(Color::Cyan)),
        Span::raw(": Refresh"),
    ]));
    frame.render_widget(help, area);
}

fn short_digest(digest: &str) -> String {
    if digest.len() > 12 {
        format!("{}...{}", &digest[..6], &digest[digest.len() - 4..])
    } else {
        digest.to_string()
    }
}

fn format_balance_changes(changes: &[TxBalanceChange]) -> String {
    if changes.is_empty() {
        return "\u{2014}".into();
    }
    let parts: Vec<String> = changes
        .iter()
        .take(3)
        .map(|bc| {
            let coin = short_coin_type(&bc.coin_type);
            format!("{} {coin}", bc.amount)
        })
        .collect();
    let mut s = parts.join(", ");
    if changes.len() > 3 {
        s.push_str(&format!(" +{} more", changes.len() - 3));
    }
    s
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

    #[test]
    fn format_owner_variants() {
        assert_eq!(
            format_owner(&OwnerInfo::Address("0xabc".into())),
            "0xabc (Address)"
        );
        assert_eq!(
            format_owner(&OwnerInfo::Object("0xdef".into())),
            "0xdef (Object)"
        );
        assert_eq!(format_owner(&OwnerInfo::Shared), "Shared");
        assert_eq!(format_owner(&OwnerInfo::Immutable), "Immutable");
        assert_eq!(format_owner(&OwnerInfo::Unknown), "Unknown");
    }

    #[test]
    fn format_json_scalar_types() {
        assert_eq!(format_json_scalar(&serde_json::json!("hello")), "\"hello\"");
        assert_eq!(format_json_scalar(&serde_json::json!(42)), "42");
        assert_eq!(format_json_scalar(&serde_json::json!(true)), "true");
        assert_eq!(format_json_scalar(&serde_json::Value::Null), "null");
    }
}
