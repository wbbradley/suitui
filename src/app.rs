use std::{
    collections::HashMap,
    path::PathBuf,
    time::{Duration, Instant},
};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::widgets::{ListState, TableState};
use sui_sdk_types::Address;
use tokio::sync::mpsc;

const COIN_CACHE_TTL: Duration = Duration::from_secs(300);
const OBJECT_CACHE_TTL: Duration = Duration::from_secs(300);
const TX_HISTORY_CACHE_TTL: Duration = Duration::from_secs(300);

struct CoinCacheEntry {
    balances: Vec<CoinBalance>,
    error: Option<String>,
    fetched_at: Instant,
}

struct ObjectCacheEntry {
    data: ObjectData,
    error: Option<String>,
    fetched_at: Instant,
}

struct DynFieldsCacheEntry {
    fields: Vec<DynFieldInfo>,
    error: Option<String>,
    fetched_at: Instant,
}

struct TxHistoryCacheEntry {
    transactions: Vec<TransactionSummary>,
    error: Option<String>,
    fetched_at: Instant,
}

struct AddressCacheEntry {
    data: AddressData,
    error: Option<String>,
    fetched_at: Instant,
}

use crate::{
    address_fetcher::{self, AddressData, AddressFetchResult},
    coin_fetcher::{self, ChainIdResult, CoinBalance, CoinFetchResult},
    config::{Env, WalletData},
    keystore::KeyEntry,
    object_fetcher::{
        self,
        DynFieldInfo,
        DynFieldsFetchResult,
        ObjectData,
        ObjectFetchResult,
        OwnerInfo,
    },
    transaction_fetcher::{self, TransactionSummary, TxHistoryFetchResult},
    transfer_executor::{self, TransferExecuteResult, TransferParams, TransferResult},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferStep {
    SelectCoin,
    EnterRecipient,
    EnterAmount,
    Review,
    Executing,
    Complete,
}

pub struct TransferState {
    pub step: TransferStep,
    pub sender: Address,
    pub balances: Vec<CoinBalance>,
    pub coin_list_state: ListState,
    pub recipient_input: String,
    pub recipient_error: Option<String>,
    pub recipient: Option<Address>,
    pub amount_input: String,
    pub amount_error: Option<String>,
    pub amount_raw: Option<u64>,
    pub result: Option<TransferResult>,
}

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

pub enum ObjectState {
    Idle,
    Loading,
    Loaded(ObjectData),
    Error(String),
}

pub enum DynFieldsState {
    Idle,
    Loading,
    Loaded(Vec<DynFieldInfo>),
    Error(String),
}

pub enum TxHistoryState {
    Idle,
    Loading,
    Loaded(Vec<TransactionSummary>),
    Error(String),
}

pub enum AddressState {
    Idle,
    Loading,
    Loaded(AddressData),
    Error(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InspectTarget {
    Object(Address),
    Address(Address),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    Main,
    Inspector(InspectTarget),
    TransactionHistory(Address),
}

pub enum AppAction {
    Quit,
    Redraw,
    None,
}

pub struct App {
    pub view_stack: Vec<View>,
    pub address_input_open: bool,
    pub address_input: String,
    pub address_input_error: Option<String>,
    pub accounts: Vec<(Address, String)>,
    pub envs: Vec<Env>,
    pub keystore: Vec<KeyEntry>,

    pub active_address: Option<Address>,
    pub active_env: Option<String>,
    pub config_path: PathBuf,

    pub focus: Focus,
    pub account_list_state: TableState,
    pub env_dropdown_open: bool,
    pub env_list_state: ListState,

    pub should_quit: bool,

    pub coin_state: CoinState,
    coin_cache: HashMap<(Address, String), CoinCacheEntry>,
    coin_inflight: Option<(Address, String)>,
    coin_displayed_key: Option<(Address, String)>,
    coin_tx: mpsc::UnboundedSender<CoinFetchResult>,
    pub coin_rx: mpsc::UnboundedReceiver<CoinFetchResult>,

    pub chain_id_cache: HashMap<String, String>,
    pub chain_id_fetch_pending: Option<String>,
    chain_id_tx: mpsc::UnboundedSender<ChainIdResult>,
    pub chain_id_rx: mpsc::UnboundedReceiver<ChainIdResult>,

    pub inspector_sel: usize,
    pub object_state: ObjectState,
    pub dyn_fields_state: DynFieldsState,
    object_cache: HashMap<(Address, String), ObjectCacheEntry>,
    object_inflight: Option<(Address, String)>,
    object_displayed_key: Option<(Address, String)>,
    object_tx: mpsc::UnboundedSender<ObjectFetchResult>,
    pub object_rx: mpsc::UnboundedReceiver<ObjectFetchResult>,
    dyn_fields_cache: HashMap<(Address, String), DynFieldsCacheEntry>,
    dyn_fields_inflight: Option<(Address, String)>,
    dyn_fields_displayed_key: Option<(Address, String)>,
    dyn_fields_tx: mpsc::UnboundedSender<DynFieldsFetchResult>,
    pub dyn_fields_rx: mpsc::UnboundedReceiver<DynFieldsFetchResult>,

    pub tx_history_state: TxHistoryState,
    pub tx_history_table_state: TableState,
    tx_history_cache: HashMap<(Address, String), TxHistoryCacheEntry>,
    tx_history_inflight: Option<(Address, String)>,
    tx_history_displayed_key: Option<(Address, String)>,
    tx_history_tx: mpsc::UnboundedSender<TxHistoryFetchResult>,
    pub tx_history_rx: mpsc::UnboundedReceiver<TxHistoryFetchResult>,

    pub address_state: AddressState,
    address_cache: HashMap<(Address, String), AddressCacheEntry>,
    address_inflight: Option<(Address, String)>,
    address_displayed_key: Option<(Address, String)>,
    address_fetch_tx: mpsc::UnboundedSender<AddressFetchResult>,
    pub address_fetch_rx: mpsc::UnboundedReceiver<AddressFetchResult>,

    pub transfer_state: Option<TransferState>,
    pub transfer_error_flash: Option<String>,

    transfer_exec_tx: mpsc::UnboundedSender<TransferExecuteResult>,
    pub transfer_exec_rx: mpsc::UnboundedReceiver<TransferExecuteResult>,
}

impl App {
    pub fn new(data: WalletData) -> Self {
        let accounts: Vec<(Address, String)> = data
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

        let keystore = match &data.keystore_path {
            Some(path) => crate::keystore::load_keystore(path).unwrap_or_else(|e| {
                eprintln!("warning: failed to load keystore: {e}");
                Vec::new()
            }),
            None => Vec::new(),
        };

        let (coin_tx, coin_rx) = mpsc::unbounded_channel();
        let (chain_id_tx, chain_id_rx) = mpsc::unbounded_channel();
        let (object_tx, object_rx) = mpsc::unbounded_channel();
        let (dyn_fields_tx, dyn_fields_rx) = mpsc::unbounded_channel();
        let (tx_history_tx, tx_history_rx) = mpsc::unbounded_channel();
        let (address_fetch_tx, address_fetch_rx) = mpsc::unbounded_channel();
        let (transfer_exec_tx, transfer_exec_rx) = mpsc::unbounded_channel();

        App {
            view_stack: vec![View::Main],
            address_input_open: false,
            address_input: String::new(),
            address_input_error: None,
            active_address: data.active_address,
            active_env: data.active_env,
            config_path: data.config_path,
            accounts,
            envs: data.envs,
            keystore,
            focus: Focus::Accounts,
            account_list_state,
            env_dropdown_open: false,
            env_list_state,
            should_quit: false,
            coin_state: CoinState::Idle,
            coin_cache: HashMap::new(),
            coin_inflight: None,
            coin_displayed_key: None,
            coin_tx,
            coin_rx,
            chain_id_cache: HashMap::new(),
            chain_id_fetch_pending: None,
            chain_id_tx,
            chain_id_rx,
            inspector_sel: 0,
            object_state: ObjectState::Idle,
            dyn_fields_state: DynFieldsState::Idle,
            object_cache: HashMap::new(),
            object_inflight: None,
            object_displayed_key: None,
            object_tx,
            object_rx,
            dyn_fields_cache: HashMap::new(),
            dyn_fields_inflight: None,
            dyn_fields_displayed_key: None,
            dyn_fields_tx,
            dyn_fields_rx,
            tx_history_state: TxHistoryState::Idle,
            tx_history_table_state: TableState::default(),
            tx_history_cache: HashMap::new(),
            tx_history_inflight: None,
            tx_history_displayed_key: None,
            tx_history_tx,
            tx_history_rx,
            address_state: AddressState::Idle,
            address_cache: HashMap::new(),
            address_inflight: None,
            address_displayed_key: None,
            address_fetch_tx,
            address_fetch_rx,
            transfer_state: None,
            transfer_error_flash: None,
            transfer_exec_tx,
            transfer_exec_rx,
        }
    }

    pub fn current_view(&self) -> View {
        *self.view_stack.last().expect("view stack is empty")
    }

    pub fn push_view(&mut self, view: View) {
        self.view_stack.push(view);
    }

    pub fn pop_view(&mut self) -> bool {
        if self.view_stack.len() > 1 {
            self.view_stack.pop();
            true
        } else {
            self.should_quit = true;
            false
        }
    }

    pub fn key_for_address(&self, addr: &Address) -> Option<&KeyEntry> {
        self.keystore.iter().find(|k| k.address == *addr)
    }

    pub fn selected_account_address(&self) -> Option<Address> {
        self.account_list_state
            .selected()
            .and_then(|i| self.accounts.get(i))
            .map(|(addr, _)| *addr)
    }

    pub fn active_env_info(&self) -> Option<&Env> {
        let env_name = self.active_env.as_ref()?;
        self.envs.iter().find(|e| e.alias == *env_name)
    }

    pub fn inspector_links(&self) -> Vec<InspectTarget> {
        match self.current_view() {
            View::Inspector(InspectTarget::Object(_)) => self.object_inspector_links(),
            View::Inspector(InspectTarget::Address(_)) => self.address_inspector_links(),
            _ => Vec::new(),
        }
    }

    fn object_inspector_links(&self) -> Vec<InspectTarget> {
        let mut links = Vec::new();
        let ObjectState::Loaded(data) = &self.object_state else {
            return links;
        };
        match &data.owner {
            OwnerInfo::Address(a) => {
                if let Ok(addr) = a.parse::<Address>() {
                    links.push(InspectTarget::Address(addr));
                }
            }
            OwnerInfo::Object(a) => {
                if let Ok(addr) = a.parse::<Address>() {
                    links.push(InspectTarget::Object(addr));
                }
            }
            _ => {}
        }
        if let DynFieldsState::Loaded(fields) = &self.dyn_fields_state {
            for f in fields {
                if let Some(id) = &f.child_id
                    && let Ok(addr) = id.parse::<Address>()
                {
                    links.push(InspectTarget::Object(addr));
                }
            }
        }
        links
    }

    fn address_inspector_links(&self) -> Vec<InspectTarget> {
        let mut links = Vec::new();
        let AddressState::Loaded(data) = &self.address_state else {
            return links;
        };
        for obj in &data.owned_objects {
            if let Ok(addr) = obj.object_id.parse::<Address>() {
                links.push(InspectTarget::Object(addr));
            }
        }
        links
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> AppAction {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.should_quit = true;
            return AppAction::Quit;
        }
        match self.current_view() {
            View::Main => self.handle_main_key(key),
            View::Inspector(InspectTarget::Object(_)) => self.handle_object_inspector_key(key),
            View::Inspector(InspectTarget::Address(_)) => self.handle_address_inspector_key(key),
            View::TransactionHistory(_) => self.handle_tx_history_key(key),
        }
    }

    fn handle_main_key(&mut self, key: KeyEvent) -> AppAction {
        self.transfer_error_flash = None;

        if self.transfer_state.is_some() {
            return self.handle_transfer_key(key);
        }
        if self.address_input_open {
            return self.handle_address_input_key(key);
        }
        if self.env_dropdown_open {
            return self.handle_env_dropdown_key(key);
        }

        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => {
                self.pop_view();
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
                    self.coin_displayed_key = None;
                    crate::config::save_active_state(
                        &self.config_path,
                        self.active_address,
                        self.active_env.as_deref(),
                    );
                }
                AppAction::Redraw
            }
            KeyCode::Char('i') => {
                self.address_input_open = true;
                self.address_input.clear();
                self.address_input_error = None;
                AppAction::Redraw
            }
            KeyCode::Char('r') => {
                if self.focus == Focus::Accounts || self.focus == Focus::Coins {
                    self.force_refresh_coins();
                }
                AppAction::Redraw
            }
            KeyCode::Char('t') => {
                if let Some(addr) = self.selected_account_address() {
                    self.tx_history_table_state.select(Some(0));
                    self.push_view(View::TransactionHistory(addr));
                }
                AppAction::Redraw
            }
            KeyCode::Char('s') => {
                if let Err(msg) = self.open_transfer() {
                    self.transfer_error_flash = Some(msg.to_string());
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
                self.coin_displayed_key = None;
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

    fn handle_address_input_key(&mut self, key: KeyEvent) -> AppAction {
        match key.code {
            KeyCode::Esc => {
                self.address_input_open = false;
                self.address_input.clear();
                self.address_input_error = None;
                AppAction::Redraw
            }
            KeyCode::Enter => match self.address_input.parse::<Address>() {
                Ok(addr) => {
                    self.address_input_open = false;
                    self.address_input.clear();
                    self.address_input_error = None;
                    self.push_view(View::Inspector(InspectTarget::Object(addr)));
                    AppAction::Redraw
                }
                Err(e) => {
                    self.address_input_error = Some(e.to_string());
                    AppAction::Redraw
                }
            },
            KeyCode::Backspace => {
                self.address_input.pop();
                self.address_input_error = None;
                AppAction::Redraw
            }
            KeyCode::Char(c) => {
                self.address_input.push(c);
                self.address_input_error = None;
                AppAction::Redraw
            }
            _ => AppAction::None,
        }
    }

    fn handle_object_inspector_key(&mut self, key: KeyEvent) -> AppAction {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                self.pop_view();
                AppAction::Redraw
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.move_inspector_selection(-1);
                AppAction::Redraw
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.move_inspector_selection(1);
                AppAction::Redraw
            }
            KeyCode::Enter => {
                self.inspect_selected_link();
                AppAction::Redraw
            }
            KeyCode::Char('r') => {
                self.force_refresh_object();
                AppAction::Redraw
            }
            _ => AppAction::None,
        }
    }

    fn force_refresh_object(&mut self) {
        let View::Inspector(InspectTarget::Object(addr)) = self.current_view() else {
            return;
        };
        let Some(env) = self.active_env_info() else {
            return;
        };
        let key = (addr, env.rpc.clone());
        self.inspector_sel = 0;
        self.object_cache.remove(&key);
        self.object_inflight = None;
        self.object_displayed_key = None;
        self.dyn_fields_cache.remove(&key);
        self.dyn_fields_inflight = None;
        self.dyn_fields_displayed_key = None;
    }

    fn move_inspector_selection(&mut self, delta: i32) {
        let count = self.inspector_links().len();
        if count == 0 {
            return;
        }
        let current = self.inspector_sel as i32;
        self.inspector_sel = (current + delta).rem_euclid(count as i32) as usize;
    }

    fn inspect_selected_link(&mut self) {
        let links = self.inspector_links();
        if let Some(&target) = links.get(self.inspector_sel) {
            self.inspector_sel = 0;
            self.push_view(View::Inspector(target));
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

    fn current_coin_key(&self) -> Option<(Address, String)> {
        let addr = self.selected_account_address()?;
        let env = self.active_env_info()?;
        Some((addr, env.rpc.clone()))
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
        let Some(key) = self.current_coin_key() else {
            self.coin_state = CoinState::Idle;
            self.coin_displayed_key = None;
            return;
        };
        if self.coin_displayed_key.as_ref() == Some(&key) {
            return; // already showing this key
        }
        // Check cache
        if let Some(entry) = self.coin_cache.get(&key)
            && entry.fetched_at.elapsed() < COIN_CACHE_TTL
        {
            self.coin_state = if let Some(msg) = &entry.error {
                CoinState::Error(msg.clone())
            } else {
                CoinState::Loaded(entry.balances.clone())
            };
            self.coin_displayed_key = Some(key);
            return;
        }
        // Check inflight
        if self.coin_inflight.as_ref() == Some(&key) {
            self.coin_state = CoinState::Loading;
            self.coin_displayed_key = Some(key);
            return;
        }
        // Spawn fetch
        self.coin_inflight = Some(key.clone());
        self.coin_displayed_key = Some(key.clone());
        self.coin_state = CoinState::Loading;
        crate::coin_fetcher::spawn_fetch(key.0, key.1, self.coin_tx.clone());
    }

    pub fn handle_coin_result(&mut self, result: CoinFetchResult) {
        let key = (result.address, result.rpc_url);
        if self.coin_inflight.as_ref() == Some(&key) {
            self.coin_inflight = None;
        }
        let entry = match &result.outcome {
            Ok(balances) => CoinCacheEntry {
                balances: balances.clone(),
                error: None,
                fetched_at: Instant::now(),
            },
            Err(msg) => CoinCacheEntry {
                balances: vec![],
                error: Some(msg.clone()),
                fetched_at: Instant::now(),
            },
        };
        self.coin_cache.insert(key.clone(), entry);
        // Update display if this result matches what we're currently viewing
        if self.coin_displayed_key.as_ref() == Some(&key) {
            match result.outcome {
                Ok(balances) => self.coin_state = CoinState::Loaded(balances),
                Err(msg) => self.coin_state = CoinState::Error(msg),
            }
        }
    }

    fn force_refresh_coins(&mut self) {
        if let Some(key) = self.current_coin_key() {
            self.coin_cache.remove(&key);
            self.coin_inflight = None;
            self.coin_displayed_key = None;
        }
    }

    pub fn maybe_trigger_object_fetch(&mut self) {
        let View::Inspector(InspectTarget::Object(addr)) = self.current_view() else {
            return;
        };
        let Some(env) = self.active_env_info() else {
            self.object_state = ObjectState::Idle;
            self.object_displayed_key = None;
            return;
        };
        let key = (addr, env.rpc.clone());
        if self.object_displayed_key.as_ref() == Some(&key) {
            return;
        }
        if let Some(entry) = self.object_cache.get(&key)
            && entry.fetched_at.elapsed() < OBJECT_CACHE_TTL
        {
            self.object_state = if let Some(msg) = &entry.error {
                ObjectState::Error(msg.clone())
            } else {
                ObjectState::Loaded(entry.data.clone())
            };
            self.object_displayed_key = Some(key);
            return;
        }
        if self.object_inflight.as_ref() == Some(&key) {
            self.object_state = ObjectState::Loading;
            self.object_displayed_key = Some(key);
            return;
        }
        self.object_inflight = Some(key.clone());
        self.object_displayed_key = Some(key.clone());
        self.object_state = ObjectState::Loading;
        self.dyn_fields_state = DynFieldsState::Idle;
        self.dyn_fields_displayed_key = None;
        object_fetcher::spawn_object_fetch(addr, key.1, self.object_tx.clone());
    }

    pub fn handle_object_result(&mut self, result: ObjectFetchResult) {
        let key = (result.object_id, result.rpc_url);
        if self.object_inflight.as_ref() == Some(&key) {
            self.object_inflight = None;
        }
        let entry = match &result.outcome {
            Ok(data) => ObjectCacheEntry {
                data: data.clone(),
                error: None,
                fetched_at: Instant::now(),
            },
            Err(msg) => ObjectCacheEntry {
                data: ObjectData::empty(),
                error: Some(msg.clone()),
                fetched_at: Instant::now(),
            },
        };
        self.object_cache.insert(key.clone(), entry);
        if self.object_displayed_key.as_ref() == Some(&key) {
            match result.outcome {
                Ok(data) => self.object_state = ObjectState::Loaded(data),
                Err(msg) => self.object_state = ObjectState::Error(msg),
            }
        }
    }

    pub fn maybe_trigger_dyn_fields_fetch(&mut self) {
        let View::Inspector(InspectTarget::Object(addr)) = self.current_view() else {
            return;
        };
        if !matches!(self.object_state, ObjectState::Loaded(_)) {
            return;
        }
        let Some(env) = self.active_env_info() else {
            return;
        };
        let key = (addr, env.rpc.clone());
        if self.dyn_fields_displayed_key.as_ref() == Some(&key) {
            return;
        }
        if let Some(entry) = self.dyn_fields_cache.get(&key)
            && entry.fetched_at.elapsed() < OBJECT_CACHE_TTL
        {
            self.dyn_fields_state = if let Some(msg) = &entry.error {
                DynFieldsState::Error(msg.clone())
            } else {
                DynFieldsState::Loaded(entry.fields.clone())
            };
            self.dyn_fields_displayed_key = Some(key);
            return;
        }
        if self.dyn_fields_inflight.as_ref() == Some(&key) {
            self.dyn_fields_state = DynFieldsState::Loading;
            self.dyn_fields_displayed_key = Some(key);
            return;
        }
        self.dyn_fields_inflight = Some(key.clone());
        self.dyn_fields_displayed_key = Some(key.clone());
        self.dyn_fields_state = DynFieldsState::Loading;
        object_fetcher::spawn_dyn_fields_fetch(addr, key.1, self.dyn_fields_tx.clone());
    }

    pub fn handle_dyn_fields_result(&mut self, result: DynFieldsFetchResult) {
        let key = (result.parent_id, result.rpc_url);
        if self.dyn_fields_inflight.as_ref() == Some(&key) {
            self.dyn_fields_inflight = None;
        }
        let entry = match &result.outcome {
            Ok(fields) => DynFieldsCacheEntry {
                fields: fields.clone(),
                error: None,
                fetched_at: Instant::now(),
            },
            Err(msg) => DynFieldsCacheEntry {
                fields: vec![],
                error: Some(msg.clone()),
                fetched_at: Instant::now(),
            },
        };
        self.dyn_fields_cache.insert(key.clone(), entry);
        if self.dyn_fields_displayed_key.as_ref() == Some(&key) {
            match result.outcome {
                Ok(fields) => self.dyn_fields_state = DynFieldsState::Loaded(fields),
                Err(msg) => self.dyn_fields_state = DynFieldsState::Error(msg),
            }
        }
    }

    fn handle_address_inspector_key(&mut self, key: KeyEvent) -> AppAction {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                self.pop_view();
                AppAction::Redraw
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.move_inspector_selection(-1);
                AppAction::Redraw
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.move_inspector_selection(1);
                AppAction::Redraw
            }
            KeyCode::Enter => {
                self.inspect_selected_link();
                AppAction::Redraw
            }
            KeyCode::Char('r') => {
                self.force_refresh_address();
                AppAction::Redraw
            }
            _ => AppAction::None,
        }
    }

    fn force_refresh_address(&mut self) {
        let View::Inspector(InspectTarget::Address(addr)) = self.current_view() else {
            return;
        };
        let Some(env) = self.active_env_info() else {
            return;
        };
        let key = (addr, env.rpc.clone());
        self.inspector_sel = 0;
        self.address_cache.remove(&key);
        self.address_inflight = None;
        self.address_displayed_key = None;
    }

    pub fn maybe_trigger_address_fetch(&mut self) {
        let View::Inspector(InspectTarget::Address(addr)) = self.current_view() else {
            return;
        };
        let Some(env) = self.active_env_info() else {
            self.address_state = AddressState::Idle;
            self.address_displayed_key = None;
            return;
        };
        let key = (addr, env.rpc.clone());
        if self.address_displayed_key.as_ref() == Some(&key) {
            return;
        }
        if let Some(entry) = self.address_cache.get(&key)
            && entry.fetched_at.elapsed() < OBJECT_CACHE_TTL
        {
            self.address_state = if let Some(msg) = &entry.error {
                AddressState::Error(msg.clone())
            } else {
                AddressState::Loaded(entry.data.clone())
            };
            self.address_displayed_key = Some(key);
            return;
        }
        if self.address_inflight.as_ref() == Some(&key) {
            self.address_state = AddressState::Loading;
            self.address_displayed_key = Some(key);
            return;
        }
        self.address_inflight = Some(key.clone());
        self.address_displayed_key = Some(key.clone());
        self.address_state = AddressState::Loading;
        address_fetcher::spawn_address_fetch(addr, key.1, self.address_fetch_tx.clone());
    }

    pub fn handle_address_fetch_result(&mut self, result: AddressFetchResult) {
        let key = (result.address, result.rpc_url);
        if self.address_inflight.as_ref() == Some(&key) {
            self.address_inflight = None;
        }
        let entry = match &result.outcome {
            Ok(data) => AddressCacheEntry {
                data: data.clone(),
                error: None,
                fetched_at: Instant::now(),
            },
            Err(msg) => AddressCacheEntry {
                data: AddressData::empty(),
                error: Some(msg.clone()),
                fetched_at: Instant::now(),
            },
        };
        self.address_cache.insert(key.clone(), entry);
        if self.address_displayed_key.as_ref() == Some(&key) {
            match result.outcome {
                Ok(data) => self.address_state = AddressState::Loaded(data),
                Err(msg) => self.address_state = AddressState::Error(msg),
            }
        }
    }

    fn open_transfer(&mut self) -> Result<(), &'static str> {
        let addr = self
            .selected_account_address()
            .ok_or("no selected address")?;
        self.key_for_address(&addr)
            .ok_or("no signing key for this address")?;
        let balances = match &self.coin_state {
            CoinState::Loaded(b) if !b.is_empty() => b.clone(),
            _ => return Err("no coins loaded for this address"),
        };
        let mut coin_list_state = ListState::default();
        coin_list_state.select(Some(0));
        self.transfer_state = Some(TransferState {
            step: TransferStep::SelectCoin,
            sender: addr,
            balances,
            coin_list_state,
            recipient_input: String::new(),
            recipient_error: None,
            recipient: None,
            amount_input: String::new(),
            amount_error: None,
            amount_raw: None,
            result: None,
        });
        Ok(())
    }

    fn handle_transfer_key(&mut self, key: KeyEvent) -> AppAction {
        let Some(state) = &self.transfer_state else {
            return AppAction::None;
        };
        match state.step {
            TransferStep::SelectCoin => self.handle_transfer_select_coin(key),
            TransferStep::EnterRecipient => self.handle_transfer_recipient(key),
            TransferStep::EnterAmount => self.handle_transfer_amount(key),
            TransferStep::Review => self.handle_transfer_review(key),
            TransferStep::Executing => AppAction::Redraw,
            TransferStep::Complete => self.handle_transfer_complete(key),
        }
    }

    fn handle_transfer_select_coin(&mut self, key: KeyEvent) -> AppAction {
        match key.code {
            KeyCode::Esc => {
                self.transfer_state = None;
                AppAction::Redraw
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if let Some(state) = &mut self.transfer_state {
                    let count = state.balances.len();
                    if count > 0 {
                        let current = state.coin_list_state.selected().unwrap_or(0) as i32;
                        let next = (current - 1).rem_euclid(count as i32) as usize;
                        state.coin_list_state.select(Some(next));
                    }
                }
                AppAction::Redraw
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if let Some(state) = &mut self.transfer_state {
                    let count = state.balances.len();
                    if count > 0 {
                        let current = state.coin_list_state.selected().unwrap_or(0) as i32;
                        let next = (current + 1).rem_euclid(count as i32) as usize;
                        state.coin_list_state.select(Some(next));
                    }
                }
                AppAction::Redraw
            }
            KeyCode::Enter => {
                if let Some(state) = &mut self.transfer_state
                    && state.coin_list_state.selected().is_some()
                {
                    state.step = TransferStep::EnterRecipient;
                }
                AppAction::Redraw
            }
            _ => AppAction::None,
        }
    }

    fn handle_transfer_recipient(&mut self, key: KeyEvent) -> AppAction {
        match key.code {
            KeyCode::Esc => {
                if let Some(state) = &mut self.transfer_state {
                    state.step = TransferStep::SelectCoin;
                    state.recipient_error = None;
                }
                AppAction::Redraw
            }
            KeyCode::Enter => {
                if let Some(state) = &mut self.transfer_state {
                    match state.recipient_input.parse::<Address>() {
                        Ok(addr) => {
                            state.recipient = Some(addr);
                            state.recipient_error = None;
                            state.step = TransferStep::EnterAmount;
                        }
                        Err(e) => {
                            state.recipient_error = Some(e.to_string());
                        }
                    }
                }
                AppAction::Redraw
            }
            KeyCode::Backspace => {
                if let Some(state) = &mut self.transfer_state {
                    state.recipient_input.pop();
                    state.recipient_error = None;
                }
                AppAction::Redraw
            }
            KeyCode::Char(c) => {
                if let Some(state) = &mut self.transfer_state {
                    state.recipient_input.push(c);
                    state.recipient_error = None;
                }
                AppAction::Redraw
            }
            _ => AppAction::None,
        }
    }

    fn handle_transfer_amount(&mut self, key: KeyEvent) -> AppAction {
        match key.code {
            KeyCode::Esc => {
                if let Some(state) = &mut self.transfer_state {
                    state.step = TransferStep::EnterRecipient;
                    state.amount_error = None;
                }
                AppAction::Redraw
            }
            KeyCode::Enter => {
                if let Some(state) = &mut self.transfer_state {
                    let selected_idx = state.coin_list_state.selected().unwrap_or(0);
                    let decimals = state
                        .balances
                        .get(selected_idx)
                        .map(|b| b.decimals)
                        .unwrap_or(9);
                    match coin_fetcher::parse_amount(&state.amount_input, decimals) {
                        Ok(raw) => {
                            let available = state
                                .balances
                                .get(selected_idx)
                                .map(|b| b.total_balance)
                                .unwrap_or(0);
                            if raw > available {
                                state.amount_error =
                                    Some("amount exceeds available balance".into());
                            } else {
                                state.amount_raw = Some(raw);
                                state.amount_error = None;
                                state.step = TransferStep::Review;
                            }
                        }
                        Err(e) => {
                            state.amount_error = Some(e);
                        }
                    }
                }
                AppAction::Redraw
            }
            KeyCode::Backspace => {
                if let Some(state) = &mut self.transfer_state {
                    state.amount_input.pop();
                    state.amount_error = None;
                }
                AppAction::Redraw
            }
            KeyCode::Char(c) if c.is_ascii_digit() || c == '.' => {
                if let Some(state) = &mut self.transfer_state {
                    state.amount_input.push(c);
                    state.amount_error = None;
                }
                AppAction::Redraw
            }
            _ => AppAction::None,
        }
    }

    fn handle_transfer_review(&mut self, key: KeyEvent) -> AppAction {
        match key.code {
            KeyCode::Esc => {
                if let Some(state) = &mut self.transfer_state {
                    state.step = TransferStep::EnterAmount;
                }
                AppAction::Redraw
            }
            KeyCode::Enter => {
                self.begin_transfer_execution();
                AppAction::Redraw
            }
            _ => AppAction::None,
        }
    }

    fn handle_transfer_complete(&mut self, key: KeyEvent) -> AppAction {
        match key.code {
            KeyCode::Enter | KeyCode::Esc => {
                self.transfer_state = None;
                AppAction::Redraw
            }
            _ => AppAction::None,
        }
    }

    fn begin_transfer_execution(&mut self) {
        let addr = match &self.transfer_state {
            Some(state) => state.sender,
            None => return,
        };
        let Some(key_entry) = self.keystore.iter().find(|k| k.address == addr) else {
            return;
        };
        let key_scheme = key_entry.scheme();
        let private_key_bytes = key_entry.private_key_bytes();
        let Some(env) = self.active_env_info() else {
            return;
        };
        let rpc_url = env.rpc.clone();

        let Some(state) = &mut self.transfer_state else {
            return;
        };
        let Some(recipient) = state.recipient else {
            return;
        };
        let Some(amount_raw) = state.amount_raw else {
            return;
        };
        let selected_idx = state.coin_list_state.selected().unwrap_or(0);
        let Some(coin_balance) = state.balances.get(selected_idx) else {
            return;
        };

        let params = TransferParams {
            sender: addr,
            recipient,
            coin_type: coin_balance.coin_type.clone(),
            amount_raw,
            key_scheme,
            private_key_bytes,
        };

        state.step = TransferStep::Executing;
        transfer_executor::spawn_execute_transfer(params, rpc_url, self.transfer_exec_tx.clone());
    }

    pub fn handle_transfer_exec_result(&mut self, exec_result: TransferExecuteResult) {
        let Some(state) = &mut self.transfer_state else {
            return;
        };
        state.result = Some(exec_result.result);
        state.step = TransferStep::Complete;
        // Clear coin cache so balances refresh after transfer
        self.coin_displayed_key = None;
    }

    fn handle_tx_history_key(&mut self, key: KeyEvent) -> AppAction {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                self.pop_view();
                AppAction::Redraw
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.move_tx_history_selection(-1);
                AppAction::Redraw
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.move_tx_history_selection(1);
                AppAction::Redraw
            }
            KeyCode::Char('r') => {
                self.force_refresh_tx_history();
                AppAction::Redraw
            }
            _ => AppAction::None,
        }
    }

    fn move_tx_history_selection(&mut self, delta: i32) {
        let TxHistoryState::Loaded(txs) = &self.tx_history_state else {
            return;
        };
        if txs.is_empty() {
            return;
        }
        let current = self.tx_history_table_state.selected().unwrap_or(0) as i32;
        let next = (current + delta).rem_euclid(txs.len() as i32) as usize;
        self.tx_history_table_state.select(Some(next));
    }

    fn force_refresh_tx_history(&mut self) {
        let View::TransactionHistory(addr) = self.current_view() else {
            return;
        };
        let Some(env) = self.active_env_info() else {
            return;
        };
        let key = (addr, env.rpc.clone());
        self.tx_history_cache.remove(&key);
        self.tx_history_inflight = None;
        self.tx_history_displayed_key = None;
        self.tx_history_table_state.select(Some(0));
    }

    pub fn maybe_trigger_tx_history_fetch(&mut self) {
        let View::TransactionHistory(addr) = self.current_view() else {
            return;
        };
        let Some(env) = self.active_env_info() else {
            self.tx_history_state = TxHistoryState::Idle;
            self.tx_history_displayed_key = None;
            return;
        };
        let key = (addr, env.rpc.clone());
        if self.tx_history_displayed_key.as_ref() == Some(&key) {
            return;
        }
        if let Some(entry) = self.tx_history_cache.get(&key)
            && entry.fetched_at.elapsed() < TX_HISTORY_CACHE_TTL
        {
            self.tx_history_state = if let Some(msg) = &entry.error {
                TxHistoryState::Error(msg.clone())
            } else {
                TxHistoryState::Loaded(entry.transactions.clone())
            };
            self.tx_history_displayed_key = Some(key);
            return;
        }
        if self.tx_history_inflight.as_ref() == Some(&key) {
            self.tx_history_state = TxHistoryState::Loading;
            self.tx_history_displayed_key = Some(key);
            return;
        }
        self.tx_history_inflight = Some(key.clone());
        self.tx_history_displayed_key = Some(key.clone());
        self.tx_history_state = TxHistoryState::Loading;
        transaction_fetcher::spawn_tx_history_fetch(addr, key.1, self.tx_history_tx.clone());
    }

    pub fn handle_tx_history_result(&mut self, result: TxHistoryFetchResult) {
        let key = (result.address, result.rpc_url);
        if self.tx_history_inflight.as_ref() == Some(&key) {
            self.tx_history_inflight = None;
        }
        let entry = match &result.outcome {
            Ok(txs) => TxHistoryCacheEntry {
                transactions: txs.clone(),
                error: None,
                fetched_at: Instant::now(),
            },
            Err(msg) => TxHistoryCacheEntry {
                transactions: vec![],
                error: Some(msg.clone()),
                fetched_at: Instant::now(),
            },
        };
        self.tx_history_cache.insert(key.clone(), entry);
        if self.tx_history_displayed_key.as_ref() == Some(&key) {
            match result.outcome {
                Ok(txs) => {
                    if !txs.is_empty() {
                        self.tx_history_table_state.select(Some(0));
                    }
                    self.tx_history_state = TxHistoryState::Loaded(txs);
                }
                Err(msg) => self.tx_history_state = TxHistoryState::Error(msg),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::{
        address_fetcher,
        config::{Account, Env, WalletData},
        object_fetcher::DynFieldKind,
    };

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn test_wallet_data() -> (WalletData, [Address; 3]) {
        let addrs = [
            Address::from_bytes([1u8; 32]).unwrap(),
            Address::from_bytes([2u8; 32]).unwrap(),
            Address::from_bytes([3u8; 32]).unwrap(),
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
            keystore_path: None,
        };
        (data, addrs)
    }

    fn test_app() -> (App, [Address; 3]) {
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
        assert!(app.coin_displayed_key.is_none());
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
        let (_, rpc_url) = app.coin_inflight.as_ref().unwrap();
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
        let (fetch_addr, _) = app.coin_inflight.as_ref().unwrap();
        assert_eq!(*fetch_addr, addrs[2]);
    }

    #[tokio::test]
    async fn coin_fetch_triggers_loading() {
        let (mut app, _) = test_app();
        app.maybe_trigger_coin_fetch();
        assert!(matches!(app.coin_state, CoinState::Loading));
        assert!(app.coin_inflight.is_some());
    }

    #[tokio::test]
    async fn coin_fetch_idempotent() {
        let (mut app, _) = test_app();
        app.maybe_trigger_coin_fetch();
        let key1 = app.coin_inflight.clone();
        app.maybe_trigger_coin_fetch();
        assert_eq!(app.coin_inflight, key1);
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
        app.coin_inflight = Some((addrs[1], rpc_url.clone()));
        app.coin_displayed_key = Some((addrs[1], rpc_url.clone()));
        app.coin_state = CoinState::Loading;

        app.handle_coin_result(CoinFetchResult {
            address: addrs[1],
            rpc_url,
            outcome: Ok(vec![CoinBalance {
                coin_type: "0x2::sui::SUI".into(),
                total_balance: 1_000_000_000,
                decimals: 9,
            }]),
        });
        assert!(matches!(app.coin_state, CoinState::Loaded(ref b) if b.len() == 1));
    }

    #[test]
    fn handle_coin_result_discards_stale() {
        let (mut app, addrs) = test_app();
        let rpc_url = "https://testnet.example.com".to_string();
        app.coin_inflight = Some((addrs[1], rpc_url.clone()));
        app.coin_displayed_key = Some((addrs[1], rpc_url.clone()));
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

    #[tokio::test]
    async fn coin_cache_hit_skips_fetch() {
        let (mut app, addrs) = test_app();
        let rpc_url = "https://testnet.example.com".to_string();
        let coin_key = (addrs[1], rpc_url.clone());
        app.coin_cache.insert(
            coin_key,
            CoinCacheEntry {
                balances: vec![CoinBalance {
                    coin_type: "0x2::sui::SUI".into(),
                    total_balance: 42,
                    decimals: 9,
                }],
                error: None,
                fetched_at: Instant::now(),
            },
        );
        app.maybe_trigger_coin_fetch();
        assert!(matches!(app.coin_state, CoinState::Loaded(ref b) if b.len() == 1));
        assert!(app.coin_inflight.is_none());
    }

    #[tokio::test]
    async fn coin_cache_expired_triggers_fetch() {
        let (mut app, addrs) = test_app();
        let rpc_url = "https://testnet.example.com".to_string();
        let coin_key = (addrs[1], rpc_url.clone());
        app.coin_cache.insert(
            coin_key,
            CoinCacheEntry {
                balances: vec![],
                error: None,
                fetched_at: Instant::now() - Duration::from_secs(301),
            },
        );
        app.maybe_trigger_coin_fetch();
        assert!(matches!(app.coin_state, CoinState::Loading));
        assert!(app.coin_inflight.is_some());
    }

    #[test]
    fn force_refresh_evicts_cache() {
        let (mut app, addrs) = test_app();
        let rpc_url = "https://testnet.example.com".to_string();
        let coin_key = (addrs[1], rpc_url.clone());
        app.coin_cache.insert(
            coin_key.clone(),
            CoinCacheEntry {
                balances: vec![],
                error: None,
                fetched_at: Instant::now(),
            },
        );
        app.coin_displayed_key = Some(coin_key.clone());
        app.handle_key(key(KeyCode::Char('r')));
        assert!(!app.coin_cache.contains_key(&coin_key));
        assert!(app.coin_displayed_key.is_none());
    }

    #[test]
    fn r_key_ignored_in_network_info_focus() {
        let (mut app, addrs) = test_app();
        let rpc_url = "https://testnet.example.com".to_string();
        let coin_key = (addrs[1], rpc_url.clone());
        app.coin_cache.insert(
            coin_key.clone(),
            CoinCacheEntry {
                balances: vec![],
                error: None,
                fetched_at: Instant::now(),
            },
        );
        app.focus = Focus::NetworkInfo;
        app.handle_key(key(KeyCode::Char('r')));
        assert!(app.coin_cache.contains_key(&coin_key));
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

    #[test]
    fn view_stack_starts_with_main() {
        let (app, _) = test_app();
        assert_eq!(app.current_view(), View::Main);
        assert_eq!(app.view_stack.len(), 1);
    }

    #[test]
    fn pop_view_on_single_view_quits() {
        let (mut app, _) = test_app();
        let continuing = app.pop_view();
        assert!(!continuing);
        assert!(app.should_quit);
    }

    #[test]
    fn push_and_pop_view() {
        let (mut app, _) = test_app();
        app.push_view(View::Main);
        assert_eq!(app.view_stack.len(), 2);
        let continuing = app.pop_view();
        assert!(continuing);
        assert!(!app.should_quit);
        assert_eq!(app.view_stack.len(), 1);
    }

    #[test]
    fn i_opens_address_input() {
        let (mut app, _) = test_app();
        app.handle_key(key(KeyCode::Char('i')));
        assert!(app.address_input_open);
        assert!(app.address_input.is_empty());
    }

    #[test]
    fn address_input_esc_cancels() {
        let (mut app, _) = test_app();
        app.handle_key(key(KeyCode::Char('i')));
        app.handle_key(key(KeyCode::Char('0')));
        app.handle_key(key(KeyCode::Esc));
        assert!(!app.address_input_open);
        assert!(app.address_input.is_empty());
    }

    #[test]
    fn address_input_typing_appends() {
        let (mut app, _) = test_app();
        app.handle_key(key(KeyCode::Char('i')));
        app.handle_key(key(KeyCode::Char('0')));
        app.handle_key(key(KeyCode::Char('x')));
        app.handle_key(key(KeyCode::Char('2')));
        assert_eq!(app.address_input, "0x2");
    }

    #[test]
    fn address_input_backspace_removes() {
        let (mut app, _) = test_app();
        app.handle_key(key(KeyCode::Char('i')));
        app.handle_key(key(KeyCode::Char('a')));
        app.handle_key(key(KeyCode::Char('b')));
        app.handle_key(key(KeyCode::Backspace));
        assert_eq!(app.address_input, "a");
    }

    #[test]
    fn address_input_valid_pushes_inspector() {
        let (mut app, _) = test_app();
        app.handle_key(key(KeyCode::Char('i')));
        for c in "0x2".chars() {
            app.handle_key(key(KeyCode::Char(c)));
        }
        app.handle_key(key(KeyCode::Enter));
        assert!(!app.address_input_open);
        assert_eq!(app.view_stack.len(), 2);
        assert!(matches!(
            app.current_view(),
            View::Inspector(InspectTarget::Object(_))
        ));
    }

    #[test]
    fn address_input_invalid_shows_error() {
        let (mut app, _) = test_app();
        app.handle_key(key(KeyCode::Char('i')));
        for c in "not_hex".chars() {
            app.handle_key(key(KeyCode::Char(c)));
        }
        app.handle_key(key(KeyCode::Enter));
        assert!(app.address_input_open);
        assert!(app.address_input_error.is_some());
        assert_eq!(app.view_stack.len(), 1);
    }

    #[test]
    fn object_inspector_esc_pops_back() {
        let (mut app, _) = test_app();
        let addr = Address::from_bytes([1u8; 32]).unwrap();
        app.push_view(View::Inspector(InspectTarget::Object(addr)));
        assert_eq!(app.view_stack.len(), 2);
        app.handle_key(key(KeyCode::Esc));
        assert_eq!(app.view_stack.len(), 1);
        assert_eq!(app.current_view(), View::Main);
    }

    #[test]
    fn object_inspector_q_pops_back() {
        let (mut app, _) = test_app();
        let addr = Address::from_bytes([1u8; 32]).unwrap();
        app.push_view(View::Inspector(InspectTarget::Object(addr)));
        app.handle_key(key(KeyCode::Char('q')));
        assert_eq!(app.current_view(), View::Main);
    }

    #[tokio::test]
    async fn object_inspector_triggers_fetch() {
        let (mut app, _) = test_app();
        let addr = Address::from_bytes([1u8; 32]).unwrap();
        app.push_view(View::Inspector(InspectTarget::Object(addr)));
        app.maybe_trigger_object_fetch();
        assert!(matches!(app.object_state, ObjectState::Loading));
        assert!(app.object_inflight.is_some());
    }

    #[test]
    fn object_fetch_not_triggered_on_main() {
        let (mut app, _) = test_app();
        app.maybe_trigger_object_fetch();
        assert!(matches!(app.object_state, ObjectState::Idle));
    }

    #[tokio::test]
    async fn object_fetch_idempotent() {
        let (mut app, _) = test_app();
        let addr = Address::from_bytes([1u8; 32]).unwrap();
        app.push_view(View::Inspector(InspectTarget::Object(addr)));
        app.maybe_trigger_object_fetch();
        let key1 = app.object_inflight.clone();
        app.maybe_trigger_object_fetch();
        assert_eq!(app.object_inflight, key1);
    }

    #[test]
    fn handle_object_result_updates_state() {
        let (mut app, _) = test_app();
        let addr = Address::from_bytes([1u8; 32]).unwrap();
        let rpc_url = "https://testnet.example.com".to_string();
        app.push_view(View::Inspector(InspectTarget::Object(addr)));
        app.object_inflight = Some((addr, rpc_url.clone()));
        app.object_displayed_key = Some((addr, rpc_url.clone()));
        app.object_state = ObjectState::Loading;

        app.handle_object_result(ObjectFetchResult {
            object_id: addr,
            rpc_url,
            outcome: Ok(ObjectData::empty()),
        });
        assert!(matches!(app.object_state, ObjectState::Loaded(_)));
    }

    #[test]
    fn handle_object_result_error() {
        let (mut app, _) = test_app();
        let addr = Address::from_bytes([1u8; 32]).unwrap();
        let rpc_url = "https://testnet.example.com".to_string();
        app.object_displayed_key = Some((addr, rpc_url.clone()));
        app.object_state = ObjectState::Loading;

        app.handle_object_result(ObjectFetchResult {
            object_id: addr,
            rpc_url,
            outcome: Err("not found".into()),
        });
        assert!(matches!(app.object_state, ObjectState::Error(_)));
    }

    #[test]
    fn dyn_fields_not_triggered_when_object_not_loaded() {
        let (mut app, _) = test_app();
        let addr = Address::from_bytes([1u8; 32]).unwrap();
        app.push_view(View::Inspector(InspectTarget::Object(addr)));
        app.object_state = ObjectState::Loading;
        app.maybe_trigger_dyn_fields_fetch();
        assert!(matches!(app.dyn_fields_state, DynFieldsState::Idle));
    }

    #[tokio::test]
    async fn dyn_fields_triggered_when_object_loaded() {
        let (mut app, _) = test_app();
        let addr = Address::from_bytes([1u8; 32]).unwrap();
        app.push_view(View::Inspector(InspectTarget::Object(addr)));
        app.object_state = ObjectState::Loaded(ObjectData::empty());
        app.maybe_trigger_dyn_fields_fetch();
        assert!(matches!(app.dyn_fields_state, DynFieldsState::Loading));
    }

    #[test]
    fn object_inspector_r_refreshes() {
        let (mut app, _) = test_app();
        let addr = Address::from_bytes([1u8; 32]).unwrap();
        app.push_view(View::Inspector(InspectTarget::Object(addr)));
        let rpc_url = "https://testnet.example.com".to_string();
        let obj_key = (addr, rpc_url.clone());
        app.object_displayed_key = Some(obj_key.clone());
        app.object_state = ObjectState::Loaded(ObjectData::empty());
        app.handle_key(key(KeyCode::Char('r')));
        assert!(app.object_displayed_key.is_none());
    }

    #[test]
    fn inspector_links_from_loaded_object() {
        let (mut app, _) = test_app();
        let addr = Address::from_bytes([1u8; 32]).unwrap();
        let owner_addr = Address::from_bytes([2u8; 32]).unwrap();
        let child_addr = Address::from_bytes([3u8; 32]).unwrap();
        app.push_view(View::Inspector(InspectTarget::Object(addr)));
        app.object_state = ObjectState::Loaded(ObjectData {
            owner: OwnerInfo::Address(owner_addr.to_string()),
            ..ObjectData::empty()
        });
        app.dyn_fields_state = DynFieldsState::Loaded(vec![
            DynFieldInfo {
                field_id: "f1".into(),
                kind: DynFieldKind::Object,
                value_type: "SomeType".into(),
                child_id: Some(child_addr.to_string()),
            },
            DynFieldInfo {
                field_id: "f2".into(),
                kind: DynFieldKind::Field,
                value_type: "Other".into(),
                child_id: None,
            },
        ]);
        let links = app.inspector_links();
        assert_eq!(links.len(), 2);
        assert_eq!(links[0], InspectTarget::Address(owner_addr));
        assert_eq!(links[1], InspectTarget::Object(child_addr));
    }

    #[test]
    fn inspector_links_empty_when_not_loaded() {
        let (mut app, _) = test_app();
        let addr = Address::from_bytes([1u8; 32]).unwrap();
        app.push_view(View::Inspector(InspectTarget::Object(addr)));
        app.object_state = ObjectState::Loading;
        assert!(app.inspector_links().is_empty());
    }

    #[test]
    fn move_inspector_selection_wraps() {
        let (mut app, _) = test_app();
        let addr = Address::from_bytes([1u8; 32]).unwrap();
        let owner_addr = Address::from_bytes([2u8; 32]).unwrap();
        let child_addr = Address::from_bytes([3u8; 32]).unwrap();
        app.push_view(View::Inspector(InspectTarget::Object(addr)));
        app.object_state = ObjectState::Loaded(ObjectData {
            owner: OwnerInfo::Address(owner_addr.to_string()),
            ..ObjectData::empty()
        });
        app.dyn_fields_state = DynFieldsState::Loaded(vec![DynFieldInfo {
            field_id: "f1".into(),
            kind: DynFieldKind::Object,
            value_type: "T".into(),
            child_id: Some(child_addr.to_string()),
        }]);
        // 2 links: owner + child
        assert_eq!(app.inspector_sel, 0);
        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.inspector_sel, 1);
        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.inspector_sel, 0); // wrapped
        app.handle_key(key(KeyCode::Up));
        assert_eq!(app.inspector_sel, 1); // wrapped backwards
    }

    #[test]
    fn enter_on_inspector_link_pushes_view() {
        let (mut app, _) = test_app();
        let addr = Address::from_bytes([1u8; 32]).unwrap();
        let owner_addr = Address::from_bytes([2u8; 32]).unwrap();
        app.push_view(View::Inspector(InspectTarget::Object(addr)));
        app.object_state = ObjectState::Loaded(ObjectData {
            owner: OwnerInfo::Address(owner_addr.to_string()),
            ..ObjectData::empty()
        });
        app.dyn_fields_state = DynFieldsState::Loaded(vec![]);
        assert_eq!(app.view_stack.len(), 2);
        app.handle_key(key(KeyCode::Enter));
        assert_eq!(app.view_stack.len(), 3);
        assert_eq!(
            app.current_view(),
            View::Inspector(InspectTarget::Address(owner_addr))
        );
        assert_eq!(app.inspector_sel, 0);
    }

    #[test]
    fn enter_on_object_owner_pushes_object_inspector() {
        let (mut app, _) = test_app();
        let addr = Address::from_bytes([1u8; 32]).unwrap();
        let owner_addr = Address::from_bytes([2u8; 32]).unwrap();
        app.push_view(View::Inspector(InspectTarget::Object(addr)));
        app.object_state = ObjectState::Loaded(ObjectData {
            owner: OwnerInfo::Object(owner_addr.to_string()),
            ..ObjectData::empty()
        });
        app.dyn_fields_state = DynFieldsState::Loaded(vec![]);
        app.handle_key(key(KeyCode::Enter));
        assert_eq!(
            app.current_view(),
            View::Inspector(InspectTarget::Object(owner_addr))
        );
    }

    #[tokio::test]
    async fn address_inspector_triggers_fetch() {
        let (mut app, _) = test_app();
        let addr = Address::from_bytes([1u8; 32]).unwrap();
        app.push_view(View::Inspector(InspectTarget::Address(addr)));
        app.maybe_trigger_address_fetch();
        assert!(matches!(app.address_state, AddressState::Loading));
        assert!(app.address_inflight.is_some());
    }

    #[test]
    fn address_inspector_links_from_owned_objects() {
        let (mut app, _) = test_app();
        let addr = Address::from_bytes([1u8; 32]).unwrap();
        let obj_addr = Address::from_bytes([4u8; 32]).unwrap();
        app.push_view(View::Inspector(InspectTarget::Address(addr)));
        app.address_state = AddressState::Loaded(AddressData {
            balances: vec![],
            owned_objects: vec![address_fetcher::OwnedObjectSummary {
                object_id: obj_addr.to_string(),
                object_type: "0x2::coin::Coin<0x2::sui::SUI>".into(),
            }],
        });
        let links = app.inspector_links();
        assert_eq!(links.len(), 1);
        assert_eq!(links[0], InspectTarget::Object(obj_addr));
    }

    #[test]
    fn address_inspector_enter_pushes_object_inspector() {
        let (mut app, _) = test_app();
        let addr = Address::from_bytes([1u8; 32]).unwrap();
        let obj_addr = Address::from_bytes([4u8; 32]).unwrap();
        app.push_view(View::Inspector(InspectTarget::Address(addr)));
        app.address_state = AddressState::Loaded(AddressData {
            balances: vec![],
            owned_objects: vec![address_fetcher::OwnedObjectSummary {
                object_id: obj_addr.to_string(),
                object_type: "0x2::coin::Coin<0x2::sui::SUI>".into(),
            }],
        });
        app.handle_key(key(KeyCode::Enter));
        assert_eq!(
            app.current_view(),
            View::Inspector(InspectTarget::Object(obj_addr))
        );
    }

    // --- Transfer tests ---

    fn app_with_coins_and_key() -> (App, [Address; 3]) {
        let (mut app, addrs) = test_app();
        app.keystore = vec![KeyEntry::test_entry(addrs[1])];
        app.coin_state = CoinState::Loaded(vec![
            CoinBalance {
                coin_type: "0x2::sui::SUI".into(),
                total_balance: 5_000_000_000,
                decimals: 9,
            },
            CoinBalance {
                coin_type: "0xabc::mod::USDC".into(),
                total_balance: 100_000_000,
                decimals: 6,
            },
        ]);
        (app, addrs)
    }

    #[test]
    fn s_opens_transfer_modal() {
        let (mut app, _) = app_with_coins_and_key();
        app.handle_key(key(KeyCode::Char('s')));
        assert!(app.transfer_state.is_some());
        let state = app.transfer_state.as_ref().unwrap();
        assert_eq!(state.step, TransferStep::SelectCoin);
        assert_eq!(state.balances.len(), 2);
    }

    #[test]
    fn s_guard_no_coins() {
        let (mut app, _) = test_app();
        app.keystore = vec![KeyEntry::test_entry(
            Address::from_bytes([2u8; 32]).unwrap(),
        )];
        // coin_state is Idle by default
        app.handle_key(key(KeyCode::Char('s')));
        assert!(app.transfer_state.is_none());
        assert!(app.transfer_error_flash.is_some());
    }

    #[test]
    fn s_guard_no_key() {
        let (mut app, _) = test_app();
        app.coin_state = CoinState::Loaded(vec![CoinBalance {
            coin_type: "0x2::sui::SUI".into(),
            total_balance: 1_000_000_000,
            decimals: 9,
        }]);
        // keystore is empty by default
        app.handle_key(key(KeyCode::Char('s')));
        assert!(app.transfer_state.is_none());
        assert!(app.transfer_error_flash.is_some());
    }

    #[test]
    fn transfer_select_coin_esc_closes() {
        let (mut app, _) = app_with_coins_and_key();
        app.handle_key(key(KeyCode::Char('s')));
        assert!(app.transfer_state.is_some());
        app.handle_key(key(KeyCode::Esc));
        assert!(app.transfer_state.is_none());
    }

    #[test]
    fn transfer_select_coin_navigate() {
        let (mut app, _) = app_with_coins_and_key();
        app.handle_key(key(KeyCode::Char('s')));
        assert_eq!(
            app.transfer_state
                .as_ref()
                .unwrap()
                .coin_list_state
                .selected(),
            Some(0)
        );
        app.handle_key(key(KeyCode::Down));
        assert_eq!(
            app.transfer_state
                .as_ref()
                .unwrap()
                .coin_list_state
                .selected(),
            Some(1)
        );
        app.handle_key(key(KeyCode::Down));
        assert_eq!(
            app.transfer_state
                .as_ref()
                .unwrap()
                .coin_list_state
                .selected(),
            Some(0)
        ); // wraps
    }

    #[test]
    fn transfer_select_coin_enter_advances() {
        let (mut app, _) = app_with_coins_and_key();
        app.handle_key(key(KeyCode::Char('s')));
        app.handle_key(key(KeyCode::Enter));
        assert_eq!(
            app.transfer_state.as_ref().unwrap().step,
            TransferStep::EnterRecipient
        );
    }

    #[test]
    fn transfer_recipient_typing() {
        let (mut app, _) = app_with_coins_and_key();
        app.handle_key(key(KeyCode::Char('s')));
        app.handle_key(key(KeyCode::Enter)); // select coin
        app.handle_key(key(KeyCode::Char('0')));
        app.handle_key(key(KeyCode::Char('x')));
        app.handle_key(key(KeyCode::Char('a')));
        assert_eq!(app.transfer_state.as_ref().unwrap().recipient_input, "0xa");
    }

    #[test]
    fn transfer_recipient_valid_advances() {
        let (mut app, _) = app_with_coins_and_key();
        app.handle_key(key(KeyCode::Char('s')));
        app.handle_key(key(KeyCode::Enter)); // select coin
        for c in "0x2".chars() {
            app.handle_key(key(KeyCode::Char(c)));
        }
        app.handle_key(key(KeyCode::Enter));
        let state = app.transfer_state.as_ref().unwrap();
        assert_eq!(state.step, TransferStep::EnterAmount);
        assert!(state.recipient.is_some());
    }

    #[test]
    fn transfer_recipient_invalid_shows_error() {
        let (mut app, _) = app_with_coins_and_key();
        app.handle_key(key(KeyCode::Char('s')));
        app.handle_key(key(KeyCode::Enter)); // select coin
        for c in "not_hex".chars() {
            app.handle_key(key(KeyCode::Char(c)));
        }
        app.handle_key(key(KeyCode::Enter));
        let state = app.transfer_state.as_ref().unwrap();
        assert_eq!(state.step, TransferStep::EnterRecipient);
        assert!(state.recipient_error.is_some());
    }

    #[test]
    fn transfer_amount_valid_advances() {
        let (mut app, _) = app_with_coins_and_key();
        app.handle_key(key(KeyCode::Char('s')));
        app.handle_key(key(KeyCode::Enter)); // select coin (SUI, 5 SUI)
        for c in "0x2".chars() {
            app.handle_key(key(KeyCode::Char(c)));
        }
        app.handle_key(key(KeyCode::Enter)); // recipient
        app.handle_key(key(KeyCode::Char('1')));
        app.handle_key(key(KeyCode::Enter)); // amount = 1 SUI
        let state = app.transfer_state.as_ref().unwrap();
        assert_eq!(state.step, TransferStep::Review);
        assert_eq!(state.amount_raw, Some(1_000_000_000));
    }

    #[test]
    fn transfer_amount_exceeds_balance() {
        let (mut app, _) = app_with_coins_and_key();
        app.handle_key(key(KeyCode::Char('s')));
        app.handle_key(key(KeyCode::Enter)); // select coin (SUI, 5 SUI)
        for c in "0x2".chars() {
            app.handle_key(key(KeyCode::Char(c)));
        }
        app.handle_key(key(KeyCode::Enter)); // recipient
        for c in "999".chars() {
            app.handle_key(key(KeyCode::Char(c)));
        }
        app.handle_key(key(KeyCode::Enter));
        let state = app.transfer_state.as_ref().unwrap();
        assert_eq!(state.step, TransferStep::EnterAmount);
        assert!(state.amount_error.is_some());
    }

    #[test]
    fn transfer_review_esc_goes_back() {
        let (mut app, _) = app_with_coins_and_key();
        app.handle_key(key(KeyCode::Char('s')));
        app.handle_key(key(KeyCode::Enter)); // select coin
        for c in "0x2".chars() {
            app.handle_key(key(KeyCode::Char(c)));
        }
        app.handle_key(key(KeyCode::Enter)); // recipient
        app.handle_key(key(KeyCode::Char('1')));
        app.handle_key(key(KeyCode::Enter)); // amount → review
        assert_eq!(
            app.transfer_state.as_ref().unwrap().step,
            TransferStep::Review
        );
        app.handle_key(key(KeyCode::Esc));
        assert_eq!(
            app.transfer_state.as_ref().unwrap().step,
            TransferStep::EnterAmount
        );
    }

    #[test]
    fn transfer_review_enter_starts_executing() {
        let (mut app, _) = app_with_coins_and_key();
        app.handle_key(key(KeyCode::Char('s')));
        app.handle_key(key(KeyCode::Enter)); // select coin
        for c in "0x2".chars() {
            app.handle_key(key(KeyCode::Char(c)));
        }
        app.handle_key(key(KeyCode::Enter)); // recipient
        app.handle_key(key(KeyCode::Char('1')));
        app.handle_key(key(KeyCode::Enter)); // amount → review
        app.handle_key(key(KeyCode::Enter)); // confirm → executing
        assert!(app.transfer_state.is_some());
        assert_eq!(
            app.transfer_state.as_ref().unwrap().step,
            TransferStep::Executing
        );
    }

    #[test]
    fn transfer_executing_blocks_keys() {
        let (mut app, _) = app_with_coins_and_key();
        app.handle_key(key(KeyCode::Char('s')));
        app.handle_key(key(KeyCode::Enter)); // select coin
        for c in "0x2".chars() {
            app.handle_key(key(KeyCode::Char(c)));
        }
        app.handle_key(key(KeyCode::Enter)); // recipient
        app.handle_key(key(KeyCode::Char('1')));
        app.handle_key(key(KeyCode::Enter)); // amount → review
        app.handle_key(key(KeyCode::Enter)); // confirm → executing

        // Keys at Executing should not change step
        app.handle_key(key(KeyCode::Esc));
        assert_eq!(
            app.transfer_state.as_ref().unwrap().step,
            TransferStep::Executing
        );
        app.handle_key(key(KeyCode::Enter));
        assert_eq!(
            app.transfer_state.as_ref().unwrap().step,
            TransferStep::Executing
        );
        app.handle_key(key(KeyCode::Char('q')));
        assert_eq!(
            app.transfer_state.as_ref().unwrap().step,
            TransferStep::Executing
        );
    }

    #[test]
    fn transfer_complete_enter_closes() {
        let (mut app, addrs) = app_with_coins_and_key();
        app.transfer_state = Some(TransferState {
            step: TransferStep::Complete,
            sender: addrs[1],
            balances: vec![],
            coin_list_state: ListState::default(),
            recipient_input: String::new(),
            recipient_error: None,
            recipient: None,
            amount_input: String::new(),
            amount_error: None,
            amount_raw: None,
            result: Some(TransferResult::Success {
                digest: "test".into(),
            }),
        });
        app.handle_key(key(KeyCode::Enter));
        assert!(app.transfer_state.is_none());
    }

    #[test]
    fn transfer_complete_esc_closes() {
        let (mut app, addrs) = app_with_coins_and_key();
        app.transfer_state = Some(TransferState {
            step: TransferStep::Complete,
            sender: addrs[1],
            balances: vec![],
            coin_list_state: ListState::default(),
            recipient_input: String::new(),
            recipient_error: None,
            recipient: None,
            amount_input: String::new(),
            amount_error: None,
            amount_raw: None,
            result: Some(TransferResult::Error("fail".into())),
        });
        app.handle_key(key(KeyCode::Esc));
        assert!(app.transfer_state.is_none());
    }

    #[test]
    fn handle_transfer_exec_result_success() {
        let (mut app, addrs) = app_with_coins_and_key();
        app.transfer_state = Some(TransferState {
            step: TransferStep::Executing,
            sender: addrs[1],
            balances: vec![],
            coin_list_state: ListState::default(),
            recipient_input: String::new(),
            recipient_error: None,
            recipient: None,
            amount_input: String::new(),
            amount_error: None,
            amount_raw: None,
            result: None,
        });
        app.handle_transfer_exec_result(TransferExecuteResult {
            result: TransferResult::Success {
                digest: "abc123".into(),
            },
        });
        let state = app.transfer_state.as_ref().unwrap();
        assert_eq!(state.step, TransferStep::Complete);
        assert!(
            matches!(&state.result, Some(TransferResult::Success { digest }) if digest == "abc123")
        );
    }

    #[test]
    fn handle_transfer_exec_result_error() {
        let (mut app, addrs) = app_with_coins_and_key();
        app.transfer_state = Some(TransferState {
            step: TransferStep::Executing,
            sender: addrs[1],
            balances: vec![],
            coin_list_state: ListState::default(),
            recipient_input: String::new(),
            recipient_error: None,
            recipient: None,
            amount_input: String::new(),
            amount_error: None,
            amount_raw: None,
            result: None,
        });
        app.handle_transfer_exec_result(TransferExecuteResult {
            result: TransferResult::Error("boom".into()),
        });
        let state = app.transfer_state.as_ref().unwrap();
        assert_eq!(state.step, TransferStep::Complete);
        assert!(matches!(&state.result, Some(TransferResult::Error(msg)) if msg == "boom"));
    }

    #[test]
    fn transfer_exec_clears_coin_cache() {
        let (mut app, addrs) = app_with_coins_and_key();
        let rpc_url = "https://testnet.example.com".to_string();
        app.coin_displayed_key = Some((addrs[1], rpc_url));
        app.transfer_state = Some(TransferState {
            step: TransferStep::Executing,
            sender: addrs[1],
            balances: vec![],
            coin_list_state: ListState::default(),
            recipient_input: String::new(),
            recipient_error: None,
            recipient: None,
            amount_input: String::new(),
            amount_error: None,
            amount_raw: None,
            result: None,
        });
        app.handle_transfer_exec_result(TransferExecuteResult {
            result: TransferResult::Success {
                digest: "xyz".into(),
            },
        });
        assert!(app.coin_displayed_key.is_none());
    }

    #[test]
    fn transfer_back_preserves_data() {
        let (mut app, _) = app_with_coins_and_key();
        app.handle_key(key(KeyCode::Char('s')));
        app.handle_key(key(KeyCode::Enter)); // select coin
        for c in "0x2".chars() {
            app.handle_key(key(KeyCode::Char(c)));
        }
        app.handle_key(key(KeyCode::Enter)); // recipient → amount
        app.handle_key(key(KeyCode::Char('1')));
        // Go back to recipient
        app.handle_key(key(KeyCode::Esc));
        let state = app.transfer_state.as_ref().unwrap();
        assert_eq!(state.step, TransferStep::EnterRecipient);
        assert_eq!(state.recipient_input, "0x2");
        assert_eq!(state.amount_input, "1");
    }

    #[test]
    fn t_uses_selected_address() {
        let (mut app, addrs) = test_app();
        // Cursor starts at addrs[1] (bob). Move to carol (index 2).
        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.selected_account_address(), Some(addrs[2]));
        // active_address is still bob
        assert_eq!(app.active_address, Some(addrs[1]));
        // Press 't' — should open tx history for carol, not bob
        app.handle_key(key(KeyCode::Char('t')));
        assert_eq!(app.current_view(), View::TransactionHistory(addrs[2]));
    }

    #[test]
    fn s_uses_selected_address() {
        let (mut app, addrs) = test_app();
        // Give carol (addrs[2]) a key and coins
        app.keystore = vec![KeyEntry::test_entry(addrs[2])];
        app.coin_state = CoinState::Loaded(vec![CoinBalance {
            coin_type: "0x2::sui::SUI".into(),
            total_balance: 1_000_000_000,
            decimals: 9,
        }]);
        // Move cursor to carol (index 2)
        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.selected_account_address(), Some(addrs[2]));
        // active_address is still bob
        assert_eq!(app.active_address, Some(addrs[1]));
        // Press 's' — should capture carol as sender
        app.handle_key(key(KeyCode::Char('s')));
        assert!(app.transfer_state.is_some());
        assert_eq!(app.transfer_state.as_ref().unwrap().sender, addrs[2]);
    }
}
