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

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::config::{Account, Env, WalletData};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn test_wallet_data() -> (WalletData, [SuiAddress; 3]) {
        let addrs = [
            SuiAddress::random_for_testing_only(),
            SuiAddress::random_for_testing_only(),
            SuiAddress::random_for_testing_only(),
        ];
        let data = WalletData {
            accounts: vec![
                Account {
                    address: addrs[0],
                    alias: "alice".into(),
                },
                Account {
                    address: addrs[1],
                    alias: "bob".into(),
                },
                Account {
                    address: addrs[2],
                    alias: "carol".into(),
                },
            ],
            envs: vec![
                Env {
                    alias: "devnet".into(),
                    rpc: "https://devnet.example.com".into(),
                    chain_id: Some("aaa".into()),
                },
                Env {
                    alias: "testnet".into(),
                    rpc: "https://testnet.example.com".into(),
                    chain_id: Some("bbb".into()),
                },
                Env {
                    alias: "mainnet".into(),
                    rpc: "https://mainnet.example.com".into(),
                    chain_id: None,
                },
            ],
            active_address: Some(addrs[1]),
            active_env: Some("testnet".into()),
            config_path: PathBuf::from("/tmp/fake"),
        };
        (data, addrs)
    }

    fn test_app() -> (App, [SuiAddress; 3]) {
        let (data, addrs) = test_wallet_data();
        (App::new(data), addrs)
    }

    #[test]
    fn focus_cycles() {
        assert_eq!(Focus::Accounts.next(), Focus::Coins);
        assert_eq!(Focus::Coins.next(), Focus::NetworkInfo);
        assert_eq!(Focus::NetworkInfo.next(), Focus::Accounts);
    }

    #[test]
    fn new_selects_active_account() {
        let (app, addrs) = test_app();
        assert_eq!(app.account_list_state.selected(), Some(1));
        assert_eq!(app.active_address, Some(addrs[1]));
        assert_eq!(app.pending_address, Some(addrs[1]));
    }

    #[test]
    fn new_selects_active_env() {
        let (app, _) = test_app();
        assert_eq!(app.env_list_state.selected(), Some(1));
        assert_eq!(app.active_env.as_deref(), Some("testnet"));
        assert_eq!(app.pending_env.as_deref(), Some("testnet"));
    }

    #[test]
    fn new_defaults_to_zero_when_no_active() {
        let (mut data, _) = test_wallet_data();
        data.active_address = None;
        data.active_env = None;
        let app = App::new(data);
        assert_eq!(app.account_list_state.selected(), Some(0));
        assert_eq!(app.env_list_state.selected(), Some(0));
    }

    #[test]
    fn no_pending_changes_initially() {
        let (app, _) = test_app();
        assert!(!app.has_pending_changes());
    }

    #[test]
    fn navigate_down_sets_pending_address() {
        let (mut app, addrs) = test_app();
        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.account_list_state.selected(), Some(2));
        assert_eq!(app.pending_address, Some(addrs[2]));
        assert!(app.has_pending_changes());
    }

    #[test]
    fn navigate_wraps_around() {
        let (mut app, addrs) = test_app();
        // Start at index 1 (bob), go down twice to wrap: 1->2->0
        app.handle_key(key(KeyCode::Down));
        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.account_list_state.selected(), Some(0));
        assert_eq!(app.pending_address, Some(addrs[0]));
    }

    #[test]
    fn navigate_up_wraps_around() {
        let (mut app, _) = test_app();
        // Start at index 1 (bob), go up twice: 1->0->2
        app.handle_key(key(KeyCode::Up));
        app.handle_key(key(KeyCode::Up));
        assert_eq!(app.account_list_state.selected(), Some(2));
    }

    #[test]
    fn vim_keys_navigate() {
        let (mut app, _) = test_app();
        app.handle_key(key(KeyCode::Char('j')));
        assert_eq!(app.account_list_state.selected(), Some(2));
        app.handle_key(key(KeyCode::Char('k')));
        assert_eq!(app.account_list_state.selected(), Some(1));
    }

    #[test]
    fn navigation_only_in_accounts_focus() {
        let (mut app, _) = test_app();
        app.focus = Focus::Coins;
        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.account_list_state.selected(), Some(1));
        assert!(!app.has_pending_changes());
    }

    #[test]
    fn tab_cycles_focus() {
        let (mut app, _) = test_app();
        assert_eq!(app.focus, Focus::Accounts);
        app.handle_key(key(KeyCode::Tab));
        assert_eq!(app.focus, Focus::Coins);
        app.handle_key(key(KeyCode::Tab));
        assert_eq!(app.focus, Focus::NetworkInfo);
        app.handle_key(key(KeyCode::Tab));
        assert_eq!(app.focus, Focus::Accounts);
    }

    #[test]
    fn q_quits() {
        let (mut app, _) = test_app();
        let action = app.handle_key(key(KeyCode::Char('q')));
        assert!(app.should_quit);
        assert!(matches!(action, AppAction::Quit));
    }

    #[test]
    fn esc_quits() {
        let (mut app, _) = test_app();
        app.handle_key(key(KeyCode::Esc));
        assert!(app.should_quit);
    }

    #[test]
    fn ctrl_c_quits() {
        let (mut app, _) = test_app();
        let action = app.handle_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
        assert!(app.should_quit);
        assert!(matches!(action, AppAction::Quit));
    }

    #[test]
    fn ctrl_c_quits_even_in_dropdown() {
        let (mut app, _) = test_app();
        app.env_dropdown_open = true;
        app.handle_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
        assert!(app.should_quit);
    }

    #[test]
    fn e_opens_env_dropdown() {
        let (mut app, _) = test_app();
        app.handle_key(key(KeyCode::Char('e')));
        assert!(app.env_dropdown_open);
    }

    #[test]
    fn esc_closes_env_dropdown() {
        let (mut app, _) = test_app();
        app.env_dropdown_open = true;
        app.handle_key(key(KeyCode::Esc));
        assert!(!app.env_dropdown_open);
        assert!(!app.should_quit);
    }

    #[test]
    fn env_dropdown_navigate_and_select() {
        let (mut app, _) = test_app();
        app.handle_key(key(KeyCode::Char('e')));
        assert!(app.env_dropdown_open);

        app.handle_key(key(KeyCode::Up));
        assert_eq!(app.env_list_state.selected(), Some(0));

        app.handle_key(key(KeyCode::Enter));
        assert!(!app.env_dropdown_open);
        assert_eq!(app.pending_env.as_deref(), Some("devnet"));
        assert!(app.has_pending_changes());
        assert_eq!(app.active_env.as_deref(), Some("testnet"));
    }

    #[test]
    fn f10_applies_pending() {
        let (mut app, addrs) = test_app();
        app.handle_key(key(KeyCode::Down)); // select carol
        app.handle_key(key(KeyCode::Char('e')));
        app.handle_key(key(KeyCode::Down)); // mainnet
        app.handle_key(key(KeyCode::Enter));

        assert!(app.has_pending_changes());

        app.handle_key(key(KeyCode::F(10)));
        assert!(!app.has_pending_changes());
        assert_eq!(app.active_address, Some(addrs[2]));
        assert_eq!(app.active_env.as_deref(), Some("mainnet"));
    }

    #[test]
    fn selected_account_address() {
        let (app, addrs) = test_app();
        assert_eq!(app.selected_account_address(), Some(addrs[1]));
    }

    #[test]
    fn active_env_info() {
        let (app, _) = test_app();
        let info = app.active_env_info().unwrap();
        assert_eq!(info.alias, "testnet");
        assert_eq!(info.chain_id.as_deref(), Some("bbb"));
    }

    #[test]
    fn empty_accounts_navigation_is_safe() {
        let (mut data, _) = test_wallet_data();
        data.accounts = vec![];
        data.active_address = None;
        let mut app = App::new(data);
        app.handle_key(key(KeyCode::Down));
        app.handle_key(key(KeyCode::Up));
        assert_eq!(app.selected_account_address(), None);
    }

    #[test]
    fn empty_envs_navigation_is_safe() {
        let (mut data, _) = test_wallet_data();
        data.envs = vec![];
        data.active_env = None;
        let mut app = App::new(data);
        app.env_dropdown_open = true;
        app.handle_key(key(KeyCode::Down));
        app.handle_key(key(KeyCode::Up));
    }
}
