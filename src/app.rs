use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::widgets::ListState;
use sui_types::base_types::SuiAddress;

use crate::config::{Env, WalletData};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Accounts,
    Coins,
    NetworkInfo,
}

impl Focus {
    pub fn next(self) -> Self {
        match self {
            Focus::Accounts => Focus::Coins,
            Focus::Coins => Focus::NetworkInfo,
            Focus::NetworkInfo => Focus::Accounts,
        }
    }
}

pub enum AppAction {
    Quit,
    Redraw,
    None,
}

pub struct App {
    pub accounts: Vec<(SuiAddress, String)>,
    pub envs: Vec<Env>,

    pub active_address: Option<SuiAddress>,
    pub active_env: Option<String>,

    pub pending_address: Option<SuiAddress>,
    pub pending_env: Option<String>,

    pub focus: Focus,
    pub account_list_state: ListState,
    pub env_dropdown_open: bool,
    pub env_list_state: ListState,

    pub should_quit: bool,
}

impl App {
    pub fn new(data: WalletData) -> Self {
        let accounts: Vec<(SuiAddress, String)> = data
            .accounts
            .into_iter()
            .map(|a| (a.address, a.alias))
            .collect();

        let active_idx = data
            .active_address
            .and_then(|addr| accounts.iter().position(|(a, _)| *a == addr))
            .unwrap_or(0);

        let env_idx = data
            .active_env
            .as_ref()
            .and_then(|env| data.envs.iter().position(|e| e.alias == *env))
            .unwrap_or(0);

        let mut account_list_state = ListState::default();
        account_list_state.select(Some(active_idx));

        let mut env_list_state = ListState::default();
        env_list_state.select(Some(env_idx));

        App {
            pending_address: data.active_address,
            pending_env: data.active_env.clone(),
            active_address: data.active_address,
            active_env: data.active_env,
            accounts,
            envs: data.envs,
            focus: Focus::Accounts,
            account_list_state,
            env_dropdown_open: false,
            env_list_state,
            should_quit: false,
        }
    }

    pub fn has_pending_changes(&self) -> bool {
        self.pending_address != self.active_address || self.pending_env != self.active_env
    }

    pub fn selected_account_address(&self) -> Option<SuiAddress> {
        self.account_list_state
            .selected()
            .and_then(|i| self.accounts.get(i))
            .map(|(addr, _)| *addr)
    }

    pub fn active_env_info(&self) -> Option<&Env> {
        let env_name = self.active_env.as_ref()?;
        self.envs.iter().find(|e| e.alias == *env_name)
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> AppAction {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.should_quit = true;
            return AppAction::Quit;
        }

        if self.env_dropdown_open {
            return self.handle_env_dropdown_key(key);
        }

        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => {
                self.should_quit = true;
                AppAction::Quit
            }
            KeyCode::Tab => {
                self.focus = self.focus.next();
                AppAction::Redraw
            }
            KeyCode::Char('e') => {
                self.env_dropdown_open = true;
                AppAction::Redraw
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if self.focus == Focus::Accounts {
                    self.move_account_selection(-1);
                }
                AppAction::Redraw
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if self.focus == Focus::Accounts {
                    self.move_account_selection(1);
                }
                AppAction::Redraw
            }
            KeyCode::F(10) => {
                self.apply_pending();
                AppAction::Redraw
            }
            _ => AppAction::None,
        }
    }

    fn handle_env_dropdown_key(&mut self, key: KeyEvent) -> AppAction {
        match key.code {
            KeyCode::Esc | KeyCode::Char('e') | KeyCode::Char('q') => {
                self.env_dropdown_open = false;
                AppAction::Redraw
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.move_env_selection(-1);
                AppAction::Redraw
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.move_env_selection(1);
                AppAction::Redraw
            }
            KeyCode::Enter => {
                if let Some(idx) = self.env_list_state.selected()
                    && let Some(env) = self.envs.get(idx)
                {
                    self.pending_env = Some(env.alias.clone());
                }
                self.env_dropdown_open = false;
                AppAction::Redraw
            }
            _ => AppAction::None,
        }
    }

    fn move_account_selection(&mut self, delta: i32) {
        if self.accounts.is_empty() {
            return;
        }
        let current = self.account_list_state.selected().unwrap_or(0) as i32;
        let next = (current + delta).rem_euclid(self.accounts.len() as i32) as usize;
        self.account_list_state.select(Some(next));
        self.pending_address = Some(self.accounts[next].0);
    }

    fn move_env_selection(&mut self, delta: i32) {
        if self.envs.is_empty() {
            return;
        }
        let current = self.env_list_state.selected().unwrap_or(0) as i32;
        let next = (current + delta).rem_euclid(self.envs.len() as i32) as usize;
        self.env_list_state.select(Some(next));
    }

    fn apply_pending(&mut self) {
        self.active_address = self.pending_address;
        self.active_env = self.pending_env.clone();
    }
}
