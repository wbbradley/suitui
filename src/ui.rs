use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Row, Table},
};
use sui_sdk_types::Address;

use crate::{
    address_fetcher::AddressData,
    app::{
        AddressState,
        App,
        CheckpointState,
        CoinState,
        DynFieldsState,
        Focus,
        GAS_BUDGET_RESERVE,
        InspectTarget,
        ObjectState,
        SpendableState,
        TransferState,
        TransferStep,
        TxDetailState,
        TxHistoryState,
        View,
    },
    checkpoint_fetcher::CheckpointData,
    coin_fetcher::{SUI_DECIMALS, format_balance, format_signed_balance, short_coin_type},
    object_fetcher::{DynFieldKind, OBJECT_NOT_FOUND, ObjectData, OwnerInfo},
    transaction_fetcher::{self, TransactionDetail, TransactionSummary, TxBalanceChange},
    transfer_executor::TransferResult,
};

pub fn draw(frame: &mut Frame, app: &mut App) {
    app.last_viewport_height = frame.area().height;
    match app.current_view() {
        View::Main => draw_main(frame, app),
        View::Inspector(InspectTarget::Object(addr)) => draw_object_inspector(frame, app, addr),
        View::Inspector(InspectTarget::Address(addr)) => {
            draw_address_inspector(frame, app, addr);
        }
        View::Inspector(InspectTarget::Transaction(ref digest)) => {
            draw_transaction_inspector(frame, app, digest.clone());
        }
        View::Inspector(InspectTarget::Checkpoint(seq)) => {
            draw_checkpoint_inspector(frame, app, seq);
        }
        View::TransactionHistory(addr) => draw_transaction_history(frame, app, addr),
    }
}

fn draw_checkpoint_inspector(frame: &mut Frame, app: &App, seq: u64) {
    let outer = Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).split(frame.area());
    let content_area = outer[0];
    let help_area = outer[1];

    let block = Block::default()
        .title(format!("Checkpoint: {seq}"))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    match &app.checkpoint_state {
        CheckpointState::Idle | CheckpointState::Loading => {
            let msg = if matches!(app.checkpoint_state, CheckpointState::Loading) {
                "Loading checkpoint..."
            } else {
                "Waiting..."
            };
            let p = Paragraph::new(format!("  {msg}"))
                .style(Style::default().fg(Color::Yellow))
                .block(block);
            frame.render_widget(p, content_area);
        }
        CheckpointState::Error(msg) => {
            let p = Paragraph::new(format!("  Error: {msg}"))
                .style(Style::default().fg(Color::Red))
                .block(block);
            frame.render_widget(p, content_area);
        }
        CheckpointState::Loaded(data) => {
            let selected = app.inspector_sel;
            let mut link_idx = 0usize;
            let mut selected_line = None;
            let mut lines = Vec::new();
            append_checkpoint_summary_lines(
                &mut lines,
                data,
                selected,
                &mut link_idx,
                &mut selected_line,
            );
            append_checkpoint_gas_lines(&mut lines, data);
            append_checkpoint_transactions_lines(
                &mut lines,
                data,
                selected,
                &mut link_idx,
                &mut selected_line,
            );

            let visible_height = content_area.height.saturating_sub(2);
            let scroll = compute_scroll(selected_line, visible_height);
            let p = Paragraph::new(lines).block(block).scroll((scroll, 0));
            frame.render_widget(p, content_area);
        }
    }

    draw_inspector_help_bar(frame, help_area);
}

fn append_checkpoint_summary_lines<'a>(
    lines: &mut Vec<Line<'a>>,
    data: &CheckpointData,
    selected: usize,
    link_idx: &mut usize,
    selected_line: &mut Option<u16>,
) {
    let label = Style::default().fg(Color::Gray);

    lines.push(Line::from(vec![
        Span::styled("  Sequence:    ", label),
        Span::raw(data.sequence_number.to_string()),
    ]));
    lines.push(Line::from(vec![
        Span::styled("  Digest:      ", label),
        Span::raw(data.digest.clone()),
    ]));
    if let Some(epoch) = data.epoch {
        lines.push(Line::from(vec![
            Span::styled("  Epoch:       ", label),
            Span::raw(epoch.to_string()),
        ]));
    }
    if let Some(ts) = &data.timestamp {
        lines.push(Line::from(vec![
            Span::styled("  Timestamp:   ", label),
            Span::raw(transaction_fetcher::format_timestamp(ts)),
        ]));
    }
    if let Some(net_txs) = data.total_network_transactions {
        lines.push(Line::from(vec![
            Span::styled("  Network Txs: ", label),
            Span::raw(net_txs.to_string()),
        ]));
    }
    lines.push(Line::from(vec![
        Span::styled("  Content Dgst:", label),
        Span::raw(format!(" {}", data.content_digest)),
    ]));
    if let Some(prev) = &data.previous_digest {
        lines.push(Line::from(vec![
            Span::styled("  Prev Digest: ", label),
            Span::raw(prev.clone()),
        ]));
    }

    // Prev Ckpt — navigable link when seq > 0
    if data.sequence_number > 0 {
        let is_selected = *link_idx == selected;
        if is_selected {
            *selected_line = Some(lines.len() as u16);
        }
        let prefix = if is_selected { "> " } else { "  " };
        let value_style = if is_selected {
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Cyan)
        };
        lines.push(Line::from(vec![
            Span::styled(format!("{prefix}Prev Ckpt:   "), label),
            Span::styled((data.sequence_number - 1).to_string(), value_style),
        ]));
        *link_idx += 1;
    }

    if data.is_end_of_epoch {
        lines.push(Line::from(Span::styled(
            "  [End of Epoch]",
            Style::default().fg(Color::Yellow),
        )));
    }
}

fn append_checkpoint_gas_lines<'a>(lines: &mut Vec<Line<'a>>, data: &CheckpointData) {
    let Some(gas) = &data.gas_summary else {
        return;
    };
    let label = Style::default().fg(Color::Gray);
    let net = (gas.computation_cost + gas.storage_cost) as i64 - gas.storage_rebate as i64;

    lines.push(Line::raw(""));
    lines.push(Line::styled(
        "  \u{2500}\u{2500} Epoch Rolling Gas \u{2500}\u{2500}",
        Style::default().fg(Color::Cyan),
    ));
    lines.push(Line::from(vec![
        Span::styled("  Computation: ", label),
        Span::raw(format_balance(gas.computation_cost, SUI_DECIMALS)),
    ]));
    lines.push(Line::from(vec![
        Span::styled("  Storage:     ", label),
        Span::raw(format_balance(gas.storage_cost, SUI_DECIMALS)),
    ]));
    lines.push(Line::from(vec![
        Span::styled("  Rebate:      ", label),
        Span::raw(format!(
            "-{}",
            format_balance(gas.storage_rebate, SUI_DECIMALS)
        )),
    ]));
    let net_str = if net < 0 {
        format!("-{}", format_balance((-net) as u64, SUI_DECIMALS))
    } else {
        format_balance(net as u64, SUI_DECIMALS)
    };
    lines.push(Line::from(vec![
        Span::styled("  Net:         ", label),
        Span::styled(net_str, Style::default().add_modifier(Modifier::BOLD)),
    ]));
}

fn append_checkpoint_transactions_lines<'a>(
    lines: &mut Vec<Line<'a>>,
    data: &CheckpointData,
    selected: usize,
    link_idx: &mut usize,
    selected_line: &mut Option<u16>,
) {
    if data.transaction_digests.is_empty() {
        return;
    }

    lines.push(Line::raw(""));
    lines.push(Line::styled(
        format!(
            "  \u{2500}\u{2500} Transactions ({}) \u{2500}\u{2500}",
            data.transaction_count
        ),
        Style::default().fg(Color::Cyan),
    ));
    for digest in &data.transaction_digests {
        let short = if digest.len() > 16 {
            format!("{}..{}", &digest[..8], &digest[digest.len() - 6..])
        } else {
            digest.clone()
        };
        let is_selected = *link_idx == selected;
        if is_selected {
            *selected_line = Some(lines.len() as u16);
        }
        let prefix = if is_selected { "> " } else { "  " };
        let value_style = if is_selected {
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Cyan)
        };
        lines.push(Line::from(vec![
            Span::raw(prefix),
            Span::styled(short, value_style),
        ]));
        *link_idx += 1;
    }
}

fn draw_transaction_inspector(frame: &mut Frame, app: &App, digest: String) {
    let outer = Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).split(frame.area());
    let content_area = outer[0];
    let help_area = outer[1];

    let short_digest = if digest.len() > 16 {
        format!("{}..{}", &digest[..8], &digest[digest.len() - 6..])
    } else {
        digest.clone()
    };
    let block = Block::default()
        .title(format!("Transaction: {short_digest}"))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    match &app.tx_detail_state {
        TxDetailState::Idle | TxDetailState::Loading => {
            let msg = if matches!(app.tx_detail_state, TxDetailState::Loading) {
                "Loading transaction..."
            } else {
                "Waiting..."
            };
            let p = Paragraph::new(format!("  {msg}"))
                .style(Style::default().fg(Color::Yellow))
                .block(block);
            frame.render_widget(p, content_area);
        }
        TxDetailState::Error(msg) => {
            let p = Paragraph::new(format!("  Error: {msg}"))
                .style(Style::default().fg(Color::Red))
                .block(block);
            frame.render_widget(p, content_area);
        }
        TxDetailState::Loaded(detail) => {
            let selected = app.inspector_sel;
            let mut link_idx = 0usize;
            let mut selected_line = None;
            let mut lines = Vec::new();
            append_tx_summary_lines(
                &mut lines,
                detail,
                selected,
                &mut link_idx,
                &mut selected_line,
            );
            append_tx_gas_lines(&mut lines, detail);
            append_tx_balance_changes_lines(&mut lines, detail);
            append_tx_changed_objects_lines(
                &mut lines,
                detail,
                selected,
                &mut link_idx,
                &mut selected_line,
            );
            append_tx_events_lines(
                &mut lines,
                detail,
                selected,
                &mut link_idx,
                &mut selected_line,
            );

            let visible_height = content_area.height.saturating_sub(2);
            let scroll = compute_scroll(selected_line, visible_height);
            let p = Paragraph::new(lines).block(block).scroll((scroll, 0));
            frame.render_widget(p, content_area);
        }
    }

    draw_inspector_help_bar(frame, help_area);
}

fn append_tx_summary_lines<'a>(
    lines: &mut Vec<Line<'a>>,
    detail: &TransactionDetail,
    selected: usize,
    link_idx: &mut usize,
    selected_line: &mut Option<u16>,
) {
    let label = Style::default().fg(Color::Gray);

    lines.push(Line::from(vec![
        Span::styled("  Digest:     ", label),
        Span::raw(detail.digest.clone()),
    ]));

    let ts_str = detail
        .timestamp
        .as_ref()
        .map(transaction_fetcher::format_timestamp)
        .unwrap_or_else(|| "\u{2014}".into());
    lines.push(Line::from(vec![
        Span::styled("  Timestamp:  ", label),
        Span::raw(ts_str),
    ]));

    if let Some(cp) = detail.checkpoint {
        let is_selected = *link_idx == selected;
        if is_selected {
            *selected_line = Some(lines.len() as u16);
        }
        let prefix = if is_selected { "> " } else { "  " };
        let value_style = if is_selected {
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Cyan)
        };
        lines.push(Line::from(vec![
            Span::styled(format!("{prefix}Checkpoint: "), label),
            Span::styled(cp.to_string(), value_style),
        ]));
        *link_idx += 1;
    } else {
        lines.push(Line::from(vec![
            Span::styled("  Checkpoint: ", label),
            Span::raw("\u{2014}"),
        ]));
    }

    // Sender is a navigable link (Address)
    let sender_linkable = detail.sender.parse::<Address>().is_ok();
    if sender_linkable {
        let is_selected = *link_idx == selected;
        if is_selected {
            *selected_line = Some(lines.len() as u16);
        }
        let prefix = if is_selected { "> " } else { "  " };
        let value_style = if is_selected {
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Cyan)
        };
        lines.push(Line::from(vec![
            Span::styled(format!("{prefix}Sender:     "), label),
            Span::styled(detail.sender.clone(), value_style),
        ]));
        *link_idx += 1;
    } else {
        lines.push(Line::from(vec![
            Span::styled("  Sender:     ", label),
            Span::raw(detail.sender.clone()),
        ]));
    }

    let status_str = match detail.success {
        Some(true) => "OK",
        Some(false) => "FAIL",
        None => "?",
    };
    let status_style = match detail.success {
        Some(true) => Style::default().fg(Color::Green),
        Some(false) => Style::default().fg(Color::Red),
        None => Style::default().fg(Color::Yellow),
    };
    lines.push(Line::from(vec![
        Span::styled("  Status:     ", label),
        Span::styled(status_str, status_style),
    ]));
}

fn append_tx_gas_lines<'a>(lines: &mut Vec<Line<'a>>, detail: &TransactionDetail) {
    let Some(gas) = &detail.gas_used else {
        return;
    };
    let label = Style::default().fg(Color::Gray);
    let net = (gas.computation_cost + gas.storage_cost) as i64 - gas.storage_rebate as i64;

    lines.push(Line::raw(""));
    lines.push(Line::styled(
        "  \u{2500}\u{2500} Gas \u{2500}\u{2500}",
        Style::default().fg(Color::Cyan),
    ));
    lines.push(Line::from(vec![
        Span::styled("  Computation: ", label),
        Span::raw(format_balance(gas.computation_cost, SUI_DECIMALS)),
    ]));
    lines.push(Line::from(vec![
        Span::styled("  Storage:     ", label),
        Span::raw(format_balance(gas.storage_cost, SUI_DECIMALS)),
    ]));
    lines.push(Line::from(vec![
        Span::styled("  Rebate:      ", label),
        Span::raw(format!(
            "-{}",
            format_balance(gas.storage_rebate, SUI_DECIMALS)
        )),
    ]));
    let net_str = if net < 0 {
        format!("-{}", format_balance((-net) as u64, SUI_DECIMALS))
    } else {
        format_balance(net as u64, SUI_DECIMALS)
    };
    lines.push(Line::from(vec![
        Span::styled("  Net:         ", label),
        Span::styled(net_str, Style::default().add_modifier(Modifier::BOLD)),
    ]));
}

fn append_tx_balance_changes_lines<'a>(lines: &mut Vec<Line<'a>>, detail: &TransactionDetail) {
    if detail.balance_changes.is_empty() {
        return;
    }

    lines.push(Line::raw(""));
    lines.push(Line::styled(
        format!(
            "  \u{2500}\u{2500} Balance Changes ({}) \u{2500}\u{2500}",
            detail.balance_changes.len()
        ),
        Style::default().fg(Color::Cyan),
    ));
    for bc in &detail.balance_changes {
        let coin = short_coin_type(&bc.coin_type);
        let amount = format_signed_balance(&bc.amount, bc.decimals);
        let amount_style = if bc.amount.starts_with('-') {
            Style::default().fg(Color::Red)
        } else {
            Style::default().fg(Color::Green)
        };
        lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(coin.to_string(), Style::default().fg(Color::Yellow)),
            Span::raw("  "),
            Span::styled(amount, amount_style),
        ]));
    }
}

fn append_tx_changed_objects_lines<'a>(
    lines: &mut Vec<Line<'a>>,
    detail: &TransactionDetail,
    selected: usize,
    link_idx: &mut usize,
    selected_line: &mut Option<u16>,
) {
    if detail.changed_objects.is_empty() {
        return;
    }

    lines.push(Line::raw(""));
    lines.push(Line::styled(
        format!(
            "  \u{2500}\u{2500} Changed Objects ({}) \u{2500}\u{2500}",
            detail.changed_objects.len()
        ),
        Style::default().fg(Color::Cyan),
    ));
    for obj in &detail.changed_objects {
        let is_linkable = obj.object_id.parse::<Address>().is_ok();
        if is_linkable {
            let is_selected = *link_idx == selected;
            if is_selected {
                *selected_line = Some(lines.len() as u16);
            }
            let prefix = if is_selected { "> " } else { "  " };
            let id_style = if is_selected {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Cyan)
            };
            let short_type = short_coin_type(&obj.object_type);
            lines.push(Line::from(vec![
                Span::raw(prefix.to_string()),
                Span::styled(obj.object_id.clone(), id_style),
                Span::raw(format!("  {} ({})", short_type, obj.id_operation)),
            ]));
            *link_idx += 1;
        } else {
            let short_type = short_coin_type(&obj.object_type);
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::raw(obj.object_id.clone()),
                Span::raw(format!("  {} ({})", short_type, obj.id_operation)),
            ]));
        }
    }
}

fn append_tx_events_lines<'a>(
    lines: &mut Vec<Line<'a>>,
    detail: &TransactionDetail,
    selected: usize,
    link_idx: &mut usize,
    selected_line: &mut Option<u16>,
) {
    if detail.events.is_empty() {
        return;
    }

    // Collect changed object IDs for dedup (matching tx_inspector_links logic)
    let changed_ids: Vec<&str> = detail
        .changed_objects
        .iter()
        .map(|o| o.object_id.as_str())
        .collect();

    // Track seen senders for dedup (initialized with tx-level sender)
    let mut seen_senders: Vec<&str> = Vec::new();
    if detail.sender.parse::<Address>().is_ok() {
        seen_senders.push(detail.sender.as_str());
    }

    lines.push(Line::raw(""));
    lines.push(Line::styled(
        format!(
            "  \u{2500}\u{2500} Events ({}) \u{2500}\u{2500}",
            detail.events.len()
        ),
        Style::default().fg(Color::Cyan),
    ));
    for (i, evt) in detail.events.iter().enumerate() {
        if i > 0 {
            lines.push(Line::raw(""));
        }

        // Package ID is a navigable link if parseable and NOT already in changed_objects
        let pkg_is_link = evt.package_id.parse::<Address>().is_ok()
            && !changed_ids.contains(&evt.package_id.as_str());
        if pkg_is_link {
            let is_selected = *link_idx == selected;
            if is_selected {
                *selected_line = Some(lines.len() as u16);
            }
            let prefix = if is_selected { "> " } else { "  " };
            let id_style = if is_selected {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Cyan)
            };
            lines.push(Line::from(vec![
                Span::raw(prefix.to_string()),
                Span::styled(evt.package_id.clone(), id_style),
                Span::raw(format!("::{}", evt.module)),
            ]));
            *link_idx += 1;
        } else {
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled(
                    format!("{}::{}", evt.package_id, evt.module),
                    Style::default().fg(Color::DarkGray),
                ),
            ]));
        }

        let sender_is_link =
            evt.sender.parse::<Address>().is_ok() && !seen_senders.contains(&evt.sender.as_str());
        if sender_is_link {
            seen_senders.push(evt.sender.as_str());
            let is_selected = *link_idx == selected;
            if is_selected {
                *selected_line = Some(lines.len() as u16);
            }
            let prefix = if is_selected { ">   " } else { "    " };
            let value_style = if is_selected {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Cyan)
            };
            lines.push(Line::from(vec![
                Span::styled(prefix, Style::default().fg(Color::Gray)),
                Span::styled("Sender: ", Style::default().fg(Color::Gray)),
                Span::styled(evt.sender.clone(), value_style),
            ]));
            *link_idx += 1;
        } else if evt.sender.parse::<Address>().is_ok() {
            lines.push(Line::from(vec![
                Span::raw("    "),
                Span::styled(
                    format!("Sender: {}", evt.sender),
                    Style::default().fg(Color::DarkGray),
                ),
            ]));
        }

        lines.push(Line::from(vec![
            Span::raw("    "),
            Span::styled(
                shorten_package_ids(&evt.event_type),
                Style::default().fg(Color::Yellow),
            ),
        ]));
        if let Some(json) = &evt.json {
            render_json_lines(lines, json, 6);
        }
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
    if app.transfer_state.is_some() {
        draw_transfer_modal(frame, app, frame.area());
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
        .title(format!(
            "Coins for {} on {}",
            addr_label,
            app.active_env.as_deref().unwrap_or("unknown")
        ))
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
                            format_balance(b.total_balance, b.decimals),
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

fn draw_help_bar(frame: &mut Frame, app: &mut App, area: Rect) {
    if let Some(flash) = &app.transfer_error_flash {
        let cols = Layout::horizontal([
            Constraint::Min(0),
            Constraint::Length(flash.len() as u16 + 2),
        ])
        .split(area);
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
            Span::styled("s", Style::default().fg(Color::Cyan)),
            Span::raw(": Send  "),
            Span::styled("i", Style::default().fg(Color::Cyan)),
            Span::raw(": Inspect  "),
            Span::styled("t", Style::default().fg(Color::Cyan)),
            Span::raw(": Tx History  "),
            Span::styled("r", Style::default().fg(Color::Cyan)),
            Span::raw(": Refresh"),
        ]));
        frame.render_widget(help, cols[0]);
        let err = Paragraph::new(flash.as_str()).style(Style::default().fg(Color::Red));
        frame.render_widget(err, cols[1]);
    } else {
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
            Span::styled("s", Style::default().fg(Color::Cyan)),
            Span::raw(": Send  "),
            Span::styled("i", Style::default().fg(Color::Cyan)),
            Span::raw(": Inspect  "),
            Span::styled("t", Style::default().fg(Color::Cyan)),
            Span::raw(": Tx History  "),
            Span::styled("r", Style::default().fg(Color::Cyan)),
            Span::raw(": Refresh"),
        ]));
        frame.render_widget(help, area);
    }
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
        .title("Inspect")
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

    let help = Paragraph::new("Object ID / Address, Tx Digest, or Checkpoint #")
        .style(Style::default().fg(Color::DarkGray));
    let help_area = Rect::new(
        inner.x,
        inner.y + inner.height.saturating_sub(1),
        inner.width,
        1,
    );
    frame.render_widget(help, help_area);
}

const TRANSFER_HEADER_LINES: u16 = 3; // 2 content lines + 1 blank separator

fn transfer_header_lines(state: &TransferState) -> Vec<Line<'static>> {
    let from_line = Line::from(vec![
        Span::styled("From:    ", Style::default().fg(Color::Gray)),
        Span::raw(format!(
            "{} ({})",
            state.sender_alias,
            short_address(&state.sender)
        )),
    ]);
    let network_style = if state.is_mainnet {
        Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };
    let network_line = Line::from(vec![
        Span::styled("Network: ", Style::default().fg(Color::Gray)),
        Span::styled(
            format!("{} ({})", state.env_name, state.chain_id),
            network_style,
        ),
    ]);
    vec![from_line, network_line]
}

fn render_transfer_header(frame: &mut Frame, state: &TransferState, area: Rect) {
    let header = Paragraph::new(transfer_header_lines(state));
    let header_area = Rect::new(area.x, area.y, area.width, 2);
    frame.render_widget(header, header_area);
}

fn draw_transfer_modal(frame: &mut Frame, app: &mut App, area: Rect) {
    let Some(state) = &app.transfer_state else {
        return;
    };
    match state.step {
        TransferStep::SelectCoin => draw_transfer_select_coin(frame, app, area),
        TransferStep::SelectRecipient => draw_transfer_recipient(frame, app, area),
        TransferStep::EnterAmount => draw_transfer_amount(frame, app, area),
        TransferStep::Review => draw_transfer_review(frame, app, area),
        TransferStep::Executing => draw_transfer_executing(frame, app, area),
        TransferStep::Complete => draw_transfer_complete(frame, app, area),
    }
}

fn draw_transfer_select_coin(frame: &mut Frame, app: &mut App, area: Rect) {
    let Some(state) = &mut app.transfer_state else {
        return;
    };
    let width = 50u16.min(area.width.saturating_sub(4));
    let height = (state.balances.len() as u16 + 4 + TRANSFER_HEADER_LINES)
        .max(1)
        .min(area.height.saturating_sub(2));
    let x = (area.width.saturating_sub(width)) / 2 + area.x;
    let y = (area.height.saturating_sub(height)) / 2 + area.y;
    let modal_area = Rect::new(x, y, width, height);

    frame.render_widget(Clear, modal_area);

    let block = Block::default()
        .title("Send — Select Coin Type (1/4)")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    let inner = block.inner(modal_area);
    frame.render_widget(block, modal_area);

    render_transfer_header(frame, state, inner);

    if state.balances.is_empty() {
        let msg = match app.coin_state {
            CoinState::Loading => "Loading coin balances...",
            CoinState::Error(_) => "Error loading coins. Press Esc to close.",
            _ => "Loading...",
        };
        let content_top = inner.y + TRANSFER_HEADER_LINES;
        let msg_area = Rect::new(
            inner.x,
            content_top,
            inner.width,
            inner
                .height
                .saturating_sub(TRANSFER_HEADER_LINES)
                .saturating_sub(1),
        );
        frame.render_widget(
            Paragraph::new(msg).style(Style::default().fg(Color::DarkGray)),
            msg_area,
        );
        let help = Paragraph::new("Esc: Cancel").style(Style::default().fg(Color::DarkGray));
        let help_area = Rect::new(
            inner.x,
            inner.y + inner.height.saturating_sub(1),
            inner.width,
            1,
        );
        frame.render_widget(help, help_area);
        return;
    }

    let items: Vec<ListItem> = state
        .balances
        .iter()
        .map(|b| {
            let label = format!(
                "{} — {}",
                short_coin_type(&b.coin_type),
                format_balance(b.total_balance, b.decimals)
            );
            ListItem::new(label)
        })
        .collect();

    let content_top = inner.y + TRANSFER_HEADER_LINES;
    let list_height = inner
        .height
        .saturating_sub(TRANSFER_HEADER_LINES)
        .saturating_sub(1);
    let list_area = Rect::new(inner.x, content_top, inner.width, list_height);

    let list = List::new(items)
        .highlight_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("> ");

    frame.render_stateful_widget(list, list_area, &mut state.coin_list_state);

    let help =
        Paragraph::new("Enter: Select  Esc: Cancel").style(Style::default().fg(Color::DarkGray));
    let help_area = Rect::new(
        inner.x,
        inner.y + inner.height.saturating_sub(1),
        inner.width,
        1,
    );
    frame.render_widget(help, help_area);
}

fn draw_transfer_recipient(frame: &mut Frame, app: &mut App, area: Rect) {
    let Some(state) = &mut app.transfer_state else {
        return;
    };

    if state.recipient_external_mode {
        // External address text input mode
        let width = 60u16.min(area.width.saturating_sub(4));
        let height = (6u16 + TRANSFER_HEADER_LINES).min(area.height.saturating_sub(2));
        let x = (area.width.saturating_sub(width)) / 2 + area.x;
        let y = (area.height.saturating_sub(height)) / 2 + area.y;
        let modal_area = Rect::new(x, y, width, height);

        frame.render_widget(Clear, modal_area);

        let block = Block::default()
            .title("Send — Recipient (2/4)")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan));

        let inner = block.inner(modal_area);
        frame.render_widget(block, modal_area);

        render_transfer_header(frame, state, inner);

        let content_top = inner.y + TRANSFER_HEADER_LINES;

        let input_line = format!("{}█", &state.recipient_input);
        let input = Paragraph::new(input_line);
        let input_area = Rect::new(inner.x, content_top, inner.width, 1);
        frame.render_widget(input, input_area);

        if let Some(err) = &state.recipient_error {
            let err_line = Paragraph::new(err.as_str()).style(Style::default().fg(Color::Red));
            let err_area = Rect::new(inner.x, content_top + 1, inner.width, 1);
            frame.render_widget(err_line, err_area);
        }

        let help = Paragraph::new("Enter: Next  Esc: Back to list")
            .style(Style::default().fg(Color::DarkGray));
        let help_area = Rect::new(
            inner.x,
            inner.y + inner.height.saturating_sub(1),
            inner.width,
            1,
        );
        frame.render_widget(help, help_area);
    } else {
        // Account list mode
        let item_count = app.accounts.len() + 1;
        let state = app.transfer_state.as_mut().unwrap();
        let width = 50u16.min(area.width.saturating_sub(4));
        let height =
            (item_count as u16 + 4 + TRANSFER_HEADER_LINES).min(area.height.saturating_sub(2));
        let x = (area.width.saturating_sub(width)) / 2 + area.x;
        let y = (area.height.saturating_sub(height)) / 2 + area.y;
        let modal_area = Rect::new(x, y, width, height);

        frame.render_widget(Clear, modal_area);

        let block = Block::default()
            .title("Send — Recipient (2/4)")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan));

        let inner = block.inner(modal_area);
        frame.render_widget(block, modal_area);

        render_transfer_header(frame, state, inner);

        let mut items: Vec<ListItem> = app
            .accounts
            .iter()
            .map(|(addr, alias)| {
                let label = format!("{} ({})", alias, short_address(addr));
                ListItem::new(label)
            })
            .collect();
        items.push(ListItem::new("External address..."));

        let content_top = inner.y + TRANSFER_HEADER_LINES;
        let list_height = inner
            .height
            .saturating_sub(TRANSFER_HEADER_LINES)
            .saturating_sub(1);
        let list_area = Rect::new(inner.x, content_top, inner.width, list_height);

        let list = List::new(items)
            .highlight_style(
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("> ");

        let state = app.transfer_state.as_mut().unwrap();
        frame.render_stateful_widget(list, list_area, &mut state.recipient_list_state);

        let help = Paragraph::new("Enter: Select  j/k: Navigate  Esc: Back")
            .style(Style::default().fg(Color::DarkGray));
        let help_area = Rect::new(
            inner.x,
            inner.y + inner.height.saturating_sub(1),
            inner.width,
            1,
        );
        frame.render_widget(help, help_area);
    }
}

fn draw_transfer_amount(frame: &mut Frame, app: &mut App, area: Rect) {
    let Some(state) = &app.transfer_state else {
        return;
    };
    let selected_idx = state.coin_list_state.selected().unwrap_or(0);
    let is_sui = state
        .balances
        .get(selected_idx)
        .map(|b| b.coin_type.ends_with("::SUI"))
        .unwrap_or(false);

    let width = 60u16.min(area.width.saturating_sub(4));
    let gas_line: u16 = if is_sui { 1 } else { 0 };
    let height = 8u16 + gas_line + TRANSFER_HEADER_LINES;
    let x = (area.width.saturating_sub(width)) / 2 + area.x;
    let y = (area.height.saturating_sub(height)) / 2 + area.y;
    let modal_area = Rect::new(x, y, width, height);

    frame.render_widget(Clear, modal_area);

    let block = Block::default()
        .title("Send — Amount (3/4)")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    let inner = block.inner(modal_area);
    frame.render_widget(block, modal_area);

    render_transfer_header(frame, state, inner);

    let content_top = inner.y + TRANSFER_HEADER_LINES;

    let coin_label = state
        .balances
        .get(selected_idx)
        .map(|b| short_coin_type(&b.coin_type).to_string())
        .unwrap_or_else(|| "?".into());

    let (available_text, available_style) = match &state.spendable_state {
        SpendableState::Loading => (
            "loading...".to_string(),
            Style::default().fg(Color::DarkGray),
        ),
        SpendableState::Loaded {
            spendable,
            coin_count,
            total_coin_count,
        } => {
            let decimals = state
                .balances
                .get(selected_idx)
                .map(|b| b.decimals)
                .unwrap_or(9);
            let formatted = format_balance(*spendable, decimals);
            let coin_info = if total_coin_count > coin_count {
                format!("{formatted} ({coin_count} of {total_coin_count} coins)")
            } else {
                format!("{formatted} ({coin_count} coins)")
            };
            (coin_info, Style::default())
        }
        SpendableState::Error(e) => (e.clone(), Style::default().fg(Color::Red)),
        SpendableState::Idle => {
            let available = state
                .balances
                .get(selected_idx)
                .map(|b| format_balance(b.total_balance, b.decimals))
                .unwrap_or_else(|| "?".into());
            (available, Style::default())
        }
    };

    let context_line = Line::from(vec![
        Span::styled(
            format!("{coin_label} \u{2014} Available: "),
            Style::default().fg(Color::Gray),
        ),
        Span::styled(available_text, available_style),
    ]);
    let context = Paragraph::new(context_line);
    let context_area = Rect::new(inner.x, content_top, inner.width, 1);
    frame.render_widget(context, context_area);

    if is_sui {
        let gas_note = Line::from(Span::styled(
            format!(
                "  {} reserved for gas",
                format_balance(GAS_BUDGET_RESERVE, SUI_DECIMALS)
            ),
            Style::default().fg(Color::DarkGray),
        ));
        let gas_area = Rect::new(inner.x, content_top + 1, inner.width, 1);
        frame.render_widget(Paragraph::new(gas_note), gas_area);
    }

    let offset = gas_line;
    let input_line = format!("{}█", &state.amount_input);
    let input = Paragraph::new(input_line);
    let input_area = Rect::new(inner.x, content_top + 2 + offset, inner.width, 1);
    frame.render_widget(input, input_area);

    if let Some(err) = &state.amount_error {
        let err_line = Paragraph::new(err.as_str()).style(Style::default().fg(Color::Red));
        let err_area = Rect::new(inner.x, content_top + 3 + offset, inner.width, 1);
        frame.render_widget(err_line, err_area);
    }

    let help = Paragraph::new("Enter: Next  Esc: Back").style(Style::default().fg(Color::DarkGray));
    let help_area = Rect::new(
        inner.x,
        inner.y + inner.height.saturating_sub(1),
        inner.width,
        1,
    );
    frame.render_widget(help, help_area);
}

fn draw_transfer_review(frame: &mut Frame, app: &mut App, area: Rect) {
    let Some(state) = &app.transfer_state else {
        return;
    };
    let width = 60u16.min(area.width.saturating_sub(4));
    let disclaimer_lines: u16 = if state.is_mainnet { 7 } else { 0 };
    let height = 10u16 + TRANSFER_HEADER_LINES + disclaimer_lines;
    let x = (area.width.saturating_sub(width)) / 2 + area.x;
    let y = (area.height.saturating_sub(height)) / 2 + area.y;
    let modal_area = Rect::new(x, y, width, height);

    frame.render_widget(Clear, modal_area);

    let block = Block::default()
        .title("Send — Review (4/4)")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(if state.is_mainnet {
            Color::Yellow
        } else {
            Color::Cyan
        }));

    let inner = block.inner(modal_area);
    frame.render_widget(block, modal_area);

    render_transfer_header(frame, state, inner);

    let content_top = inner.y + TRANSFER_HEADER_LINES;
    let label_style = Style::default().fg(Color::Gray);

    let selected_idx = state.coin_list_state.selected().unwrap_or(0);
    let selected_decimals = state
        .balances
        .get(selected_idx)
        .map(|b| b.decimals)
        .unwrap_or(SUI_DECIMALS);
    let coin_label = state
        .balances
        .get(selected_idx)
        .map(|b| short_coin_type(&b.coin_type).to_string())
        .unwrap_or("?".into());
    let recipient = state.recipient.map(|a| a.to_string()).unwrap_or("?".into());
    let amount = state
        .amount_raw
        .map(|r| format_balance(r, selected_decimals))
        .unwrap_or("?".into());

    let lines = vec![
        Line::from(vec![
            Span::styled("Coin:   ", label_style),
            Span::raw(coin_label),
        ]),
        Line::from(vec![
            Span::styled("To:     ", label_style),
            Span::raw(recipient),
        ]),
        Line::from(vec![
            Span::styled("Amount: ", label_style),
            Span::raw(amount),
        ]),
        Line::from(vec![
            Span::styled("Gas:    ", label_style),
            Span::styled(
                format!(
                    "~{} SUI (max)",
                    format_balance(GAS_BUDGET_RESERVE, SUI_DECIMALS)
                ),
                Style::default().fg(Color::DarkGray),
            ),
        ]),
    ];

    let summary = Paragraph::new(lines);
    let summary_area = Rect::new(inner.x, content_top, inner.width, 4);
    frame.render_widget(summary, summary_area);

    if state.is_mainnet {
        let warn_style = Style::default().fg(Color::Red).add_modifier(Modifier::BOLD);
        let warn_text_style = Style::default().fg(Color::Yellow);
        let disclaimer = Paragraph::new(vec![
            Line::from(""),
            Line::from(Span::styled("WARNING", warn_style)),
            Line::styled(
                "This software is provided as-is, with no warranty.",
                warn_text_style,
            ),
            Line::styled(
                "You are sending real assets on Sui mainnet.",
                warn_text_style,
            ),
            Line::styled(
                "The authors accept no liability for lost funds.",
                warn_text_style,
            ),
            Line::styled(
                "By pressing Enter you confirm you accept all risks",
                warn_text_style,
            ),
            Line::styled("and proceed at your own discretion.", warn_text_style),
        ]);
        let disclaimer_area = Rect::new(inner.x, content_top + 4, inner.width, 8);
        frame.render_widget(disclaimer, disclaimer_area);
    }

    let help_text = if state.is_mainnet {
        "Enter: Confirm & Send  Esc: Back"
    } else {
        "Enter: Send  Esc: Back"
    };
    let help = Paragraph::new(help_text).style(Style::default().fg(Color::DarkGray));
    let help_area = Rect::new(
        inner.x,
        inner.y + inner.height.saturating_sub(1),
        inner.width,
        1,
    );
    frame.render_widget(help, help_area);
}

fn draw_transfer_executing(frame: &mut Frame, app: &App, area: Rect) {
    let Some(state) = &app.transfer_state else {
        return;
    };
    let width = 50u16.min(area.width.saturating_sub(4));
    let height = 5u16 + TRANSFER_HEADER_LINES;
    let x = (area.width.saturating_sub(width)) / 2 + area.x;
    let y = (area.height.saturating_sub(height)) / 2 + area.y;
    let modal_area = Rect::new(x, y, width, height);

    frame.render_widget(Clear, modal_area);

    let block = Block::default()
        .title("Send — Executing")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow));

    let inner = block.inner(modal_area);
    frame.render_widget(block, modal_area);

    render_transfer_header(frame, state, inner);

    let content_top = inner.y + TRANSFER_HEADER_LINES;
    let content_height = inner.height.saturating_sub(TRANSFER_HEADER_LINES);

    let msg = Paragraph::new("Submitting transaction...").style(Style::default().fg(Color::Yellow));
    let msg_area = Rect::new(inner.x, content_top + content_height / 2, inner.width, 1);
    frame.render_widget(msg, msg_area);
}

fn draw_transfer_complete(frame: &mut Frame, app: &App, area: Rect) {
    let Some(state) = &app.transfer_state else {
        return;
    };
    let width = 60u16.min(area.width.saturating_sub(4));
    let height = 7u16 + TRANSFER_HEADER_LINES;
    let x = (area.width.saturating_sub(width)) / 2 + area.x;
    let y = (area.height.saturating_sub(height)) / 2 + area.y;
    let modal_area = Rect::new(x, y, width, height);

    frame.render_widget(Clear, modal_area);

    let (border_color, lines) = match &state.result {
        Some(TransferResult::Success { digest }) => (
            Color::Green,
            vec![
                Line::from(Span::styled(
                    "Transaction submitted!",
                    Style::default().fg(Color::Green),
                )),
                Line::raw(""),
                Line::from(vec![
                    Span::styled("Digest: ", Style::default().fg(Color::Gray)),
                    Span::raw(digest.clone()),
                ]),
            ],
        ),
        Some(TransferResult::Error(msg)) => (
            Color::Red,
            vec![
                Line::from(Span::styled(
                    "Transaction failed",
                    Style::default().fg(Color::Red),
                )),
                Line::raw(""),
                Line::from(Span::raw(msg.clone())),
            ],
        ),
        None => (Color::DarkGray, vec![Line::raw("No result")]),
    };

    let block = Block::default()
        .title("Send — Complete")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color));

    let inner = block.inner(modal_area);
    frame.render_widget(block, modal_area);

    render_transfer_header(frame, state, inner);

    let content_top = inner.y + TRANSFER_HEADER_LINES;

    let body = Paragraph::new(lines);
    let body_area = Rect::new(
        inner.x,
        content_top,
        inner.width,
        inner
            .height
            .saturating_sub(TRANSFER_HEADER_LINES)
            .saturating_sub(1),
    );
    frame.render_widget(body, body_area);

    let help = Paragraph::new("Enter/Esc: Close").style(Style::default().fg(Color::DarkGray));
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
            let lines = if msg == OBJECT_NOT_FOUND {
                vec![
                    Line::from(""),
                    Line::styled("  Object not found", Style::default().fg(Color::Yellow)),
                    Line::from(""),
                    Line::styled(
                        "  Sui addresses and object IDs are both 32-byte hex values",
                        Style::default().fg(Color::Gray),
                    ),
                    Line::styled(
                        "  and can look identical. This ID may be an address instead.",
                        Style::default().fg(Color::Gray),
                    ),
                    Line::from(""),
                    Line::from(vec![
                        Span::raw("  Press "),
                        Span::styled(
                            "a",
                            Style::default()
                                .fg(Color::Cyan)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::raw(" to try inspecting as an address."),
                    ]),
                ]
            } else {
                vec![Line::styled(
                    format!("  Error: {msg}"),
                    Style::default().fg(Color::Red),
                )]
            };
            let p = Paragraph::new(lines).block(block);
            frame.render_widget(p, content_area);
        }
        ObjectState::Loaded(data) => {
            let selected = app.inspector_sel;
            let mut link_idx = 0usize;
            let mut selected_line = None;
            let mut lines = Vec::new();
            append_metadata_lines(
                &mut lines,
                data,
                selected,
                &mut link_idx,
                &mut selected_line,
            );
            append_properties_lines(&mut lines, data);
            append_dyn_fields_lines(
                &mut lines,
                &app.dyn_fields_state,
                selected,
                &mut link_idx,
                &mut selected_line,
            );

            let visible_height = content_area.height.saturating_sub(2);
            let scroll = compute_scroll(selected_line, visible_height);
            let p = Paragraph::new(lines).block(block).scroll((scroll, 0));
            frame.render_widget(p, content_area);
        }
    }

    if matches!(&app.object_state, ObjectState::Error(msg) if msg == OBJECT_NOT_FOUND) {
        let help = Paragraph::new(Line::from(vec![
            Span::styled("Esc", Style::default().fg(Color::Cyan)),
            Span::raw(": Back  "),
            Span::styled("a", Style::default().fg(Color::Cyan)),
            Span::raw(": Try as Address  "),
            Span::styled("r", Style::default().fg(Color::Cyan)),
            Span::raw(": Refresh"),
        ]));
        frame.render_widget(help, help_area);
    } else {
        draw_inspector_help_bar(frame, help_area);
    }
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
    selected_line: &mut Option<u16>,
) {
    let label = Style::default().fg(Color::Gray);

    lines.push(Line::from(vec![
        Span::styled("  Type:     ", label),
        Span::raw(shorten_package_ids(&data.object_type)),
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
        if is_selected {
            *selected_line = Some(lines.len() as u16);
        }
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

    let prev_tx_linkable = !data.previous_transaction.is_empty();
    if prev_tx_linkable {
        let is_selected = *link_idx == selected;
        if is_selected {
            *selected_line = Some(lines.len() as u16);
        }
        let prefix = if is_selected { "> " } else { "  " };
        let value_style = if is_selected {
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Cyan)
        };
        lines.push(Line::from(vec![
            Span::styled(format!("{prefix}Prev Tx:  "), label),
            Span::styled(data.previous_transaction.clone(), value_style),
        ]));
        *link_idx += 1;
    } else {
        lines.push(Line::from(vec![
            Span::styled("  Prev Tx:  ", label),
            Span::raw(data.previous_transaction.clone()),
        ]));
    }
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
    selected_line: &mut Option<u16>,
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
                        if is_selected {
                            *selected_line = Some(lines.len() as u16);
                        }
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
                            Span::raw(format!(
                                "  {}  {}  ",
                                f.field_id,
                                shorten_package_ids(&f.value_type)
                            )),
                            Span::styled(child, child_style),
                        ]));
                        *link_idx += 1;
                    } else {
                        let child = f.child_id.as_deref().unwrap_or("");
                        lines.push(Line::from(vec![
                            Span::styled("  ", Style::default()),
                            Span::styled(kind_str, Style::default().fg(Color::Yellow)),
                            Span::raw(format!(
                                "  {}  {}  {}",
                                f.field_id,
                                shorten_package_ids(&f.value_type),
                                child
                            )),
                        ]));
                    }
                }
            }
        }
    }
}

/// Shorten 0x-prefixed 64-hex-char package IDs to minimal form when value <= 0xffff.
/// E.g. "0x0000000000000000000000000000000000000000000000000000000000000002" → "0x2"
fn shorten_package_ids(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if i + 66 <= bytes.len()
            && bytes[i] == b'0'
            && bytes[i + 1] == b'x'
            && bytes[i + 2..i + 66].iter().all(|b| b.is_ascii_hexdigit())
        {
            let hex_str = &s[i + 2..i + 66];
            // Check if first 60 hex chars are all zeros (value fits in 16 bits)
            if hex_str[..60].bytes().all(|b| b == b'0') {
                let tail = hex_str[60..].trim_start_matches('0');
                let short = if tail.is_empty() { "0" } else { tail };
                result.push_str("0x");
                result.push_str(short);
            } else {
                result.push_str(&s[i..i + 66]);
            }
            i += 66;
        } else {
            result.push(bytes[i] as char);
            i += 1;
        }
    }
    result
}

fn compute_scroll(selected_line: Option<u16>, visible_height: u16) -> u16 {
    let Some(sel) = selected_line else {
        return 0;
    };
    if visible_height == 0 {
        return 0;
    }
    sel.saturating_sub(visible_height / 3)
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

fn draw_address_inspector(frame: &mut Frame, app: &App, addr: Address) {
    let outer = Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).split(frame.area());
    let content_area = outer[0];
    let help_area = outer[1];

    let block = Block::default()
        .title(format!("Address Inspector: {}", addr))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    match &app.address_state {
        AddressState::Idle | AddressState::Loading => {
            let msg = if matches!(app.address_state, AddressState::Loading) {
                "Loading address data..."
            } else {
                "Waiting..."
            };
            let p = Paragraph::new(format!("  {msg}"))
                .style(Style::default().fg(Color::Yellow))
                .block(block);
            frame.render_widget(p, content_area);
        }
        AddressState::Error(msg) => {
            let p = Paragraph::new(format!("  Error: {msg}"))
                .style(Style::default().fg(Color::Red))
                .block(block);
            frame.render_widget(p, content_area);
        }
        AddressState::Loaded(data) => {
            let selected = app.inspector_sel;
            let mut link_idx = 0usize;
            let mut selected_line = None;
            let mut lines = Vec::new();
            append_address_balances_lines(&mut lines, data);
            append_owned_objects_lines(
                &mut lines,
                data,
                selected,
                &mut link_idx,
                &mut selected_line,
            );

            let visible_height = content_area.height.saturating_sub(2);
            let scroll = compute_scroll(selected_line, visible_height);
            let p = Paragraph::new(lines).block(block).scroll((scroll, 0));
            frame.render_widget(p, content_area);
        }
    }

    draw_inspector_help_bar(frame, help_area);
}

fn append_address_balances_lines<'a>(lines: &mut Vec<Line<'a>>, data: &AddressData) {
    lines.push(Line::styled(
        format!("  ── Balances ({}) ──", data.balances.len()),
        Style::default().fg(Color::Cyan),
    ));
    if data.balances.is_empty() {
        lines.push(Line::styled(
            "  (none)",
            Style::default().fg(Color::DarkGray),
        ));
    } else {
        for b in &data.balances {
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled(
                    short_coin_type(&b.coin_type).to_string(),
                    Style::default().fg(Color::Yellow),
                ),
                Span::raw(format!("  {}", format_balance(b.total_balance, b.decimals))),
            ]));
        }
    }
}

fn append_owned_objects_lines<'a>(
    lines: &mut Vec<Line<'a>>,
    data: &AddressData,
    selected: usize,
    link_idx: &mut usize,
    selected_line: &mut Option<u16>,
) {
    lines.push(Line::raw(""));
    lines.push(Line::styled(
        format!("  ── Owned Objects ({}) ──", data.owned_objects.len()),
        Style::default().fg(Color::Cyan),
    ));
    if data.owned_objects.is_empty() {
        lines.push(Line::styled(
            "  (none)",
            Style::default().fg(Color::DarkGray),
        ));
    } else {
        for obj in &data.owned_objects {
            let is_selected = *link_idx == selected;
            if is_selected {
                *selected_line = Some(lines.len() as u16);
            }
            let prefix = if is_selected { "> " } else { "  " };
            let id_style = if is_selected {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Cyan)
            };
            let short_type = short_coin_type(&obj.object_type);
            lines.push(Line::from(vec![
                Span::raw(prefix.to_string()),
                Span::styled(obj.object_id.clone(), id_style),
                Span::raw(format!("  {short_type}")),
            ]));
            *link_idx += 1;
        }
    }
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
                    format_balance(net, SUI_DECIMALS)
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
            let amount = format_signed_balance(&bc.amount, bc.decimals);
            format!("{amount} {coin}")
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

    #[test]
    fn compute_scroll_no_selection() {
        assert_eq!(compute_scroll(None, 20), 0);
    }

    #[test]
    fn compute_scroll_selected_near_top() {
        assert_eq!(compute_scroll(Some(3), 30), 0);
    }

    #[test]
    fn compute_scroll_selected_far_down() {
        assert_eq!(compute_scroll(Some(50), 30), 40);
    }

    #[test]
    fn compute_scroll_zero_height() {
        assert_eq!(compute_scroll(Some(10), 0), 0);
    }

    #[test]
    fn compute_scroll_selected_at_threshold() {
        assert_eq!(compute_scroll(Some(10), 30), 0);
        assert_eq!(compute_scroll(Some(11), 30), 1);
    }

    #[test]
    fn shorten_package_ids_basic() {
        assert_eq!(
            shorten_package_ids(
                "0x0000000000000000000000000000000000000000000000000000000000000002::coin::Coin"
            ),
            "0x2::coin::Coin"
        );
    }

    #[test]
    fn shorten_package_ids_generic() {
        let input = "0x0000000000000000000000000000000000000000000000000000000000000002::coin::Coin<0x0000000000000000000000000000000000000000000000000000000000000002::sui::SUI>";
        assert_eq!(shorten_package_ids(input), "0x2::coin::Coin<0x2::sui::SUI>");
    }

    #[test]
    fn shorten_package_ids_large_address_unchanged() {
        let input = "0x00000000000000000000000000000000000000000000000000000000000abcde::mod::Type";
        assert_eq!(shorten_package_ids(input), input);
    }

    #[test]
    fn shorten_package_ids_zero() {
        assert_eq!(
            shorten_package_ids(
                "0x0000000000000000000000000000000000000000000000000000000000000000::mod::T"
            ),
            "0x0::mod::T"
        );
    }

    #[test]
    fn shorten_package_ids_no_match() {
        assert_eq!(shorten_package_ids("bare_type"), "bare_type");
    }
}
