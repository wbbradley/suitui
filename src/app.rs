use std::{collections::HashMap, path::PathBuf};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::widgets::{ListState, TableState};
use sui_types::base_types::SuiAddress;
use tokio::sync::mpsc;

use crate::{
    coin_fetcher::{self, ChainIdResult, CoinBalance, CoinFetchResult},
    config::{Env, WalletData},
};

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

pub enum CoinState {
    Idle,
    Loading,
    Loaded(Vec<CoinBalance>),
    Error(String),
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
    pub config_path: PathBuf,

    pub focus: Focus,
    pub account_list_state: TableState,
    pub env_dropdown_open: bool,
    pub env_list_state: ListState,

    pub should_quit: bool,

    pub coin_state: CoinState,
    coin_fetch_key: Option<(SuiAddress, String)>,
    coin_tx: mpsc::UnboundedSender<CoinFetchResult>,
    pub coin_rx: mpsc::UnboundedReceiver<CoinFetchResult>,

    pub chain_id_cache: HashMap<String, String>,
    pub chain_id_fetch_pending: Option<String>,
    chain_id_tx: mpsc::UnboundedSender<ChainIdResult>,
    pub chain_id_rx: mpsc::UnboundedReceiver<ChainIdResult>,
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

        let mut account_list_state = TableState::default();
        account_list_state.select(Some(active_idx));

        let mut env_list_state = ListState::default();
        env_list_state.select(Some(env_idx));

        let (coin_tx, coin_rx) = mpsc::unbounded_channel();
        let (chain_id_tx, chain_id_rx) = mpsc::unbounded_channel();

        App {
            active_address: data.active_address,
            active_env: data.active_env,
            config_path: data.config_path,
            accounts,
            envs: data.envs,
            focus: Focus::Accounts,
            account_list_state,
            env_dropdown_open: false,
            env_list_state,
            should_quit: false,
            coin_state: CoinState::Idle,
            coin_fetch_key: None,
            coin_tx,
            coin_rx,
            chain_id_cache: HashMap::new(),
            chain_id_fetch_pending: None,
            chain_id_tx,
            chain_id_rx,
        }
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
            KeyCode::Enter => {
                if self.focus == Focus::Accounts {
                    self.active_address = self.selected_account_address();
                    self.coin_fetch_key = None;
                    crate::config::save_active_state(
                        &self.config_path,
                        self.active_address,
                        self.active_env.as_deref(),
                    );
                }
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
                    self.active_env = Some(env.alias.clone());
                }
                self.env_dropdown_open = false;
                self.coin_fetch_key = None;
                crate::config::save_active_state(
                    &self.config_path,
                    self.active_address,
                    self.active_env.as_deref(),
                );
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
    }

    fn move_env_selection(&mut self, delta: i32) {
        if self.envs.is_empty() {
            return;
        }
        let current = self.env_list_state.selected().unwrap_or(0) as i32;
        let next = (current + delta).rem_euclid(self.envs.len() as i32) as usize;
        self.env_list_state.select(Some(next));
    }

    pub fn maybe_trigger_chain_id_fetch(&mut self) {
        let Some(env) = self.active_env_info() else {
            return;
        };
        if env.chain_id.is_some() {
            return;
        }
        let rpc_url = env.rpc.clone();
        if self.chain_id_cache.contains_key(&rpc_url) {
            return;
        }
        if self.chain_id_fetch_pending.as_ref() == Some(&rpc_url) {
            return;
        }
        self.chain_id_fetch_pending = Some(rpc_url.clone());
        coin_fetcher::spawn_chain_id_fetch(rpc_url, self.chain_id_tx.clone());
    }

    pub fn handle_chain_id_result(&mut self, result: ChainIdResult) {
        if self.chain_id_fetch_pending.as_ref() == Some(&result.rpc_url) {
            self.chain_id_fetch_pending = None;
        }
        if let Ok(chain_id) = result.outcome {
            self.chain_id_cache.insert(result.rpc_url, chain_id);
        }
    }

    pub fn maybe_trigger_coin_fetch(&mut self) {
        let Some(addr) = self.selected_account_address() else {
            self.coin_state = CoinState::Idle;
            return;
        };
        let Some(env) = self.active_env_info() else {
            self.coin_state = CoinState::Idle;
            return;
        };
        let rpc_url = env.rpc.clone();
        let key = (addr, rpc_url.clone());
        if self.coin_fetch_key.as_ref() == Some(&key) {
            return;
        }
        self.coin_fetch_key = Some(key);
        self.coin_state = CoinState::Loading;
        crate::coin_fetcher::spawn_fetch(addr, rpc_url, self.coin_tx.clone());
    }

    pub fn handle_coin_result(&mut self, result: CoinFetchResult) {
        let key = (result.address, result.rpc_url);
        if self.coin_fetch_key.as_ref() != Some(&key) {
            return;
        }
        match result.outcome {
            Ok(balances) => self.coin_state = CoinState::Loaded(balances),
            Err(msg) => self.coin_state = CoinState::Error(msg),
        }
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
    }

    #[test]
    fn new_selects_active_env() {
        let (app, _) = test_app();
        assert_eq!(app.env_list_state.selected(), Some(1));
        assert_eq!(app.active_env.as_deref(), Some("testnet"));
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
    fn navigate_down_moves_cursor() {
        let (mut app, addrs) = test_app();
        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.account_list_state.selected(), Some(2));
        // active_address unchanged until Enter
        assert_eq!(app.active_address, Some(addrs[1]));
    }

    #[test]
    fn navigate_wraps_around() {
        let (mut app, _) = test_app();
        // Start at index 1 (bob), go down twice to wrap: 1->2->0
        app.handle_key(key(KeyCode::Down));
        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.account_list_state.selected(), Some(0));
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
        assert_eq!(app.active_env.as_deref(), Some("devnet"));
        assert!(app.coin_fetch_key.is_none());
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

    #[tokio::test]
    async fn coin_fetch_no_env_stays_idle() {
        let (mut data, _) = test_wallet_data();
        data.active_env = None;
        let mut app = App::new(data);
        app.maybe_trigger_coin_fetch();
        assert!(matches!(app.coin_state, CoinState::Idle));
    }

    #[tokio::test]
    async fn coin_fetch_uses_active_env() {
        let (mut app, _) = test_app();
        app.active_env = Some("devnet".into());
        app.maybe_trigger_coin_fetch();
        assert!(matches!(app.coin_state, CoinState::Loading));
        let (_, rpc_url) = app.coin_fetch_key.as_ref().unwrap();
        assert_eq!(rpc_url, "https://devnet.example.com");
    }

    #[tokio::test]
    async fn coin_fetch_uses_selected_account() {
        let (mut app, addrs) = test_app();
        // Cursor starts on bob (index 1), move to carol (index 2)
        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.selected_account_address(), Some(addrs[2]));
        // active_address is still bob
        assert_eq!(app.active_address, Some(addrs[1]));
        // Coin fetch should use carol (selected), not bob (active)
        app.maybe_trigger_coin_fetch();
        assert!(matches!(app.coin_state, CoinState::Loading));
        let (fetch_addr, _) = app.coin_fetch_key.as_ref().unwrap();
        assert_eq!(*fetch_addr, addrs[2]);
    }

    #[tokio::test]
    async fn coin_fetch_triggers_loading() {
        let (mut app, _) = test_app();
        app.maybe_trigger_coin_fetch();
        assert!(matches!(app.coin_state, CoinState::Loading));
        assert!(app.coin_fetch_key.is_some());
    }

    #[tokio::test]
    async fn coin_fetch_idempotent() {
        let (mut app, _) = test_app();
        app.maybe_trigger_coin_fetch();
        let key1 = app.coin_fetch_key.clone();
        app.maybe_trigger_coin_fetch();
        assert_eq!(app.coin_fetch_key, key1);
    }

    #[test]
    fn enter_sets_active_address() {
        let (mut app, addrs) = test_app();
        // Navigate cursor to carol (index 2)
        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.account_list_state.selected(), Some(2));
        // active_address still bob
        assert_eq!(app.active_address, Some(addrs[1]));
        // Press Enter
        app.handle_key(key(KeyCode::Enter));
        assert_eq!(app.active_address, Some(addrs[2]));
    }

    #[test]
    fn enter_in_coins_focus_does_nothing() {
        let (mut app, addrs) = test_app();
        app.focus = Focus::Coins;
        app.handle_key(key(KeyCode::Enter));
        assert_eq!(app.active_address, Some(addrs[1]));
    }

    #[test]
    fn handle_coin_result_accepts_matching() {
        let (mut app, addrs) = test_app();
        let rpc_url = "https://testnet.example.com".to_string();
        app.coin_fetch_key = Some((addrs[1], rpc_url.clone()));
        app.coin_state = CoinState::Loading;

        app.handle_coin_result(CoinFetchResult {
            address: addrs[1],
            rpc_url,
            outcome: Ok(vec![CoinBalance {
                coin_type: "0x2::sui::SUI".into(),
                total_balance: 1_000_000_000,
            }]),
        });
        assert!(matches!(app.coin_state, CoinState::Loaded(ref b) if b.len() == 1));
    }

    #[test]
    fn handle_coin_result_discards_stale() {
        let (mut app, addrs) = test_app();
        let rpc_url = "https://testnet.example.com".to_string();
        app.coin_fetch_key = Some((addrs[1], rpc_url.clone()));
        app.coin_state = CoinState::Loading;

        // Result for a different address — should be discarded
        app.handle_coin_result(CoinFetchResult {
            address: addrs[0],
            rpc_url,
            outcome: Ok(vec![]),
        });
        assert!(matches!(app.coin_state, CoinState::Loading));
    }

    #[test]
    fn chain_id_fetch_skipped_when_config_has_it() {
        let (mut app, _) = test_app();
        // testnet has chain_id: Some("bbb")
        app.active_env = Some("testnet".into());
        app.maybe_trigger_chain_id_fetch();
        assert!(app.chain_id_fetch_pending.is_none());
    }

    #[tokio::test]
    async fn chain_id_fetch_triggered_when_missing() {
        let (mut app, _) = test_app();
        // mainnet has chain_id: None
        app.active_env = Some("mainnet".into());
        app.maybe_trigger_chain_id_fetch();
        assert_eq!(
            app.chain_id_fetch_pending.as_deref(),
            Some("https://mainnet.example.com")
        );
    }

    #[tokio::test]
    async fn chain_id_fetch_idempotent() {
        let (mut app, _) = test_app();
        app.active_env = Some("mainnet".into());
        app.maybe_trigger_chain_id_fetch();
        assert!(app.chain_id_fetch_pending.is_some());
        // Second call should not re-trigger
        app.maybe_trigger_chain_id_fetch();
        assert_eq!(
            app.chain_id_fetch_pending.as_deref(),
            Some("https://mainnet.example.com")
        );
    }

    #[test]
    fn chain_id_cache_hit() {
        let (mut app, _) = test_app();
        app.active_env = Some("mainnet".into());
        app.chain_id_cache
            .insert("https://mainnet.example.com".into(), "35834a8a".into());
        app.maybe_trigger_chain_id_fetch();
        assert!(app.chain_id_fetch_pending.is_none());
    }

    #[test]
    fn handle_chain_id_result_populates_cache() {
        let (mut app, _) = test_app();
        let rpc_url = "https://mainnet.example.com".to_string();
        app.chain_id_fetch_pending = Some(rpc_url.clone());

        app.handle_chain_id_result(ChainIdResult {
            rpc_url: rpc_url.clone(),
            outcome: Ok("35834a8a".into()),
        });
        assert!(app.chain_id_fetch_pending.is_none());
        assert_eq!(app.chain_id_cache.get(&rpc_url).unwrap(), "35834a8a");
    }
}
