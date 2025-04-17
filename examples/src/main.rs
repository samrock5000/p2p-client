use hex;
use std::collections::HashSet;
use std::ops::ControlFlow;
use std::{env, io, net, thread};
mod error;
use arboard::Clipboard;
use nakamoto_cash::chain::Transaction;
use nakamoto_cash::common::bitcoin::network::constants::ServiceFlags;
use nakamoto_cash::common::bitcoin::{Txid, cash_addr};
use slint::PlatformError;
use slint::{Model, ModelRc, SharedString};
use std::rc::Rc;

slint::include_modules!();

use argh::FromArgs;
use client::{Client, Config, Event, Peer};
mod logger;
use nakamoto_cash::client::traits::Handle;
use nakamoto_cash::client::{self, Network};
use nakamoto_cash::common::bitcoin::util::bloom::{Bloom, BloomFilter};
use nakamoto_cash::p2p::Command;
use nakamoto_cash::p2p::PeerId;
type Reactor = nakamoto_cash::net::poll::Reactor<net::TcpStream>;
use crossbeam_channel::{self as chan, Receiver, Sender};
const TXID_LEN: usize = 32;

#[derive(Clone, Debug)]
pub struct MerkleScanRange {
    begin: u64,
    end: u64,
}

impl MerkleScanRange {
    pub fn new(begin: u64, end: u64) -> Self {
        Self { begin, end }
    }
}

#[derive(Clone, Debug)]
pub enum UIMessage {
    NetworkConnected(Network),
    BlocksDownloading(bool),
    HeaderLoaded(u64),
    BlockConnected(u64),
    AddBloomItem(String),
    SendLoadFilter,
    ResetFilter,
    ClearFilterAndPeers,
    PeerLoadedFilter(PeerId),
    ReceivedMatchedTx { transaction: Transaction },
    ReceivedBlock(u64),
    RequestBlocks(MerkleScanRange),
}

#[derive(Clone)]
pub struct FilterState {
    bloom: BloomFilter,
    peers: Vec<PeerId>,
    filtered_peers: Vec<(PeerId, bool)>,
    is_set: bool,
}

impl FilterState {
    pub fn new() -> Self {
        let filter = Bloom::<u8>::new_for_fp_rate(10_000, 0.01);
        Self {
            bloom: BloomFilter::from(filter),
            peers: Vec::with_capacity(32),
            filtered_peers: Vec::with_capacity(32),
            is_set: false,
        }
    }

    pub fn reset(&mut self) {
        let filter = Bloom::<u8>::new_for_fp_rate(10_000, 0.01);
        self.bloom = BloomFilter::from(filter);
        self.filtered_peers.clear();
        self.is_set = false;
    }

    pub fn add_bloom_item(&mut self, data: String) -> Result<(), error::Error> {
        let mut temp_bloom = Bloom::<u8>::new_for_fp_rate(1024, 0.01);
        if let Ok(addr_data) = cash_addr::decode(data.as_str()) {
            temp_bloom.set(&mut addr_data.0.clone());
        } else {
            match hex::decode(data) {
                Ok(mut bytes) => {
                    if TXID_LEN == bytes.len() {
                        bytes.reverse();
                        log::info!("Loading txid {:?}", hex::encode(bytes.clone()));
                    }
                    temp_bloom.set(&mut bytes);
                }
                Err(e) => log::error!("Error decoding hex: {}", e),
            }
        }
        self.bloom = BloomFilter::from(temp_bloom);
        Ok(())
    }

    pub fn send_bloom_filter<H: Handle>(
        &mut self,
        client: &H,
        peers: Vec<(PeerId, bool)>,
    ) -> Result<(), error::Error> {
        let filter = self.bloom.clone();
        self.filtered_peers.extend(peers.clone());
        let peers = peers.iter().map(|p| p.0).collect::<Vec<_>>();
        client
            .command(Command::LoadBloomFilter((filter, peers)))
            .map_err(|e| error::Error::from(e))?;
        Ok(())
    }
}

pub struct Watcher<H> {
    client: H,
    network: Network,
    filter_state: FilterState,
    txids: HashSet<Txid>,
}

impl<H: Handle> Watcher<H> {
    pub fn new(client: H, network: client::Network) -> Self {
        Self {
            client,
            network,
            filter_state: FilterState::new(),
            txids: HashSet::new(),
        }
    }

    pub fn run(
        &mut self,
        events: &Receiver<client::Event>,
        ui_input_rx: &Receiver<UIMessage>,
        ui_show_tx: &Sender<UIMessage>,
        // peers_vec_tx: &Sender<Vec<Peer>>,
        // peers_vec_rx: &Receiver<Vec<Peer>>,
    ) -> Result<(), error::Error> {
        loop {
            if let Ok(event) = ui_input_rx.try_recv() {
                if let ControlFlow::Break(()) = self.handle_user_input(event)? {
                    break;
                }
            }
            if let Ok(event) = events.try_recv() {
                if let ControlFlow::Break(()) = self.handle_client_event(event, ui_show_tx)? {
                    break;
                }
            }
        }
        Ok(())
    }

    fn handle_client_event(
        &mut self,
        event: client::Event,
        ui_show_tx: &chan::Sender<UIMessage>,
    ) -> Result<ControlFlow<()>, error::Error> {
        match event {
            Event::Ready { tip, time, .. } => {
                ui_show_tx.send(UIMessage::HeaderLoaded(tip)).unwrap();
                ui_show_tx
                    .send(UIMessage::NetworkConnected(self.network))
                    .unwrap();
                log::info!("Client Ready {:?}", time.to_string());
            }
            Event::BlockConnected { height, .. } => {
                ui_show_tx.send(UIMessage::BlockConnected(height)).unwrap();
            }
            Event::PeerNegotiated { addr, .. } => {
                self.filter_state.peers.push(addr);
            }
            Event::MerkleBlockScanStarted { peer, .. } => {
                self.filter_state.filtered_peers.iter_mut().for_each(|p| {
                      if p.0 == peer {
                        p.1 = true;
                    }
                });
                ui_show_tx.send(UIMessage::BlocksDownloading(true)).unwrap();
            }
            Event::MerkleBlockRescanStopped { peer,.. } => {
                self.filter_state.filtered_peers.iter_mut().for_each(|p| {
                    if p.0 == peer {
                        p.1 = false;
                    }
                });
                if self.filter_state.filtered_peers.iter().all(|p| !p.1) {
                    ui_show_tx
                        .send(UIMessage::BlocksDownloading(false))
                        .unwrap();
                }
            }
            Event::PeerDisconnected { addr, .. } => {
                self.filter_state.peers.retain(|peer| peer != &addr);
                self.filter_state
                    .filtered_peers
                    .retain(|peer| peer.0 != addr);
            }
            Event::ReceivedMatchedTx { transaction } => {
                self.txids.insert(transaction.txid());
                ui_show_tx
                    .send(UIMessage::ReceivedMatchedTx { transaction })
                    .unwrap();
            }
            Event::ReceivedMerkleBlock { merkle_block,height,.. } => {
                _ = merkle_block;
                ui_show_tx
                    .send(UIMessage::ReceivedBlock(height))
                    .unwrap();
                // let mut matches = self.txids.iter().cloned().collect::<Vec<_>>();
                // let mut indexes: Vec<u32> = vec![];
                // _ = merkle_block.extract_matches(&mut matches, &mut indexes);
            }
            Event::PeerLoadedBloomFilter { peer, .. } => {
                if self.filter_state.is_set {
                    ui_show_tx.send(UIMessage::PeerLoadedFilter(peer)).unwrap();
                }
            }
            _ => {}
        }
        Ok(ControlFlow::Continue(()))
    }

    fn handle_user_input(
        &mut self,
        ui_input: UIMessage,
        // peers_vec_tx: &Sender<Vec<Peer>>,
        // peers_vec_rx: &Receiver<Vec<Peer>>,
    ) -> Result<ControlFlow<()>, error::Error> {
        match ui_input {
            UIMessage::AddBloomItem(data) => {
                self.filter_state.add_bloom_item(data)?;
                self.filter_state.is_set = true;
            }
            UIMessage::SendLoadFilter => {
                // let peer_tx = peers_vec_tx.clone();
                // self.client
                //     .command(Command::GetPeers(ServiceFlags::BLOOM, peer_tx))?;
                // if let Ok(peers) = peers_vec_rx.recv() {
                if !self.filter_state.peers.is_empty() {
                    let peer_ids = self
                        .filter_state
                        .peers
                        .to_vec()
                        .iter()
                        .cloned()
                        .map(|p| (p, false))
                        .collect::<Vec<_>>();
                    self.filter_state
                        .send_bloom_filter(&self.client, peer_ids)?;
                }
                // }
            }
            UIMessage::ResetFilter => {
                self.filter_state.reset();
            }
            UIMessage::ClearFilterAndPeers => {
                self.filter_state.reset();
                self.filter_state
                    .send_bloom_filter(
                        &self.client,
                        self.filter_state
                            .peers
                            .to_vec()
                            .iter()
                            .cloned()
                            .map(|p| (p, false))
                            .collect::<Vec<_>>(),
                    )
                    .unwrap();
            }
            UIMessage::RequestBlocks(range) => {

                self.client.command(Command::MerkleBlockRescan {
                    from: std::ops::Bound::Included(range.begin),
                    to: std::ops::Bound::Included(range.end),
                    peers: self
                        .filter_state
                        .filtered_peers
                        .to_vec()
                        .iter()
                        .cloned()
                        .map(|p| (p.0))
                        .collect::<Vec<_>>(),
                })?;
            }
            _ => {}
        }
        Ok(ControlFlow::Continue(()))
    }
}

/// A Bitcoin P2P Light Client.
#[derive(FromArgs)]
pub struct Options {
    /// network to connect to, eg. `chipnet`
    #[argh(option, default = "Network::default()")]
    pub network: Network,
    /// connect to this node
    #[argh(option)]
    pub connect: Vec<net::SocketAddr>,
    /// enable debug logging
    #[argh(switch)]
    pub debug: Option<bool>,
}

fn main() {
    let opts = Options::from_env();
    let client = Client::<Reactor>::new().unwrap();
    let handle = client.handle();
    let network = opts.network;
    let connect = opts.connect;
    let shutdown_tx = handle.clone();

    let level = if opts.debug.is_some() {
        log::Level::Debug
    } else {
        log::Level::Info
    };

    let (loading_tx, _loading_rx) = chan::unbounded();
    let (ui_show_tx, ui_show_rx) = chan::unbounded();
    let (ui_input_tx, ui_input_rx) = chan::unbounded();
    // let (peers_vec_tx, peers_vec_rx) = chan::unbounded();

    let client_recv = handle.events();

    let cfg = Config {
        network,
        connect,
        listen: vec![],
        ..Config::default()
    };

    logger::init(level).expect("initializing logger for the first time");
    let t1 = thread::spawn(move || client.load(cfg, loading_tx)?.run());
    let t2 = thread::spawn(move || {
        if let Err(err) = Watcher::new(handle.clone(), network).run(
            &client_recv,
            &ui_input_rx,
            &ui_show_tx,
        ) {
            println!("FATAL ERR {:?}", err);
            std::process::exit(1);
        }
    });

    run_ui_main(&ui_input_tx, &ui_show_rx).expect("UI failed");
        shutdown_tx.shutdown().unwrap();
    _ = slint::quit_event_loop();
    t1.join().unwrap().unwrap();
    t2.join().unwrap();

}

pub fn run_ui_main(
    ui_input_tx: &Sender<UIMessage>,
    ui_show_rx: &Receiver<UIMessage>,
) -> Result<(), PlatformError> {
    let main_window = MainWindow::new()?;
    let ui_show_rx = ui_show_rx.clone();

    let app = main_window.as_weak();
    let tx_handle = ui_input_tx.clone();

    app.unwrap().on_load_bloom_item(move || {
        let filter_items = Rc::new(slint::VecModel::<SharedString>::from(vec![]));
        let items = app.unwrap().get_bloom_items();
        let item = app.unwrap().get_bloom_item();

        if !item.is_empty() {
            filter_items.push(item);
            items.iter().for_each(|f| filter_items.push(f.clone()));
            app.unwrap().set_bloom_items(filter_items.clone().into());
            filter_items.iter().for_each(|f| {
                tx_handle
                    .send(UIMessage::AddBloomItem(f.to_string()))
                    .unwrap()
            });
        }
    });
    let app = main_window.as_weak();

    let ui_input_peers_tx = ui_input_tx.clone();
    app.unwrap().on_load_peers_filter(move || {
        ui_input_peers_tx.send(UIMessage::SendLoadFilter).unwrap();
    });

    let ui_handle_tx = ui_input_tx.clone();
    app.unwrap().on_reset_filters(move || {
        ui_handle_tx.send(UIMessage::ClearFilterAndPeers).unwrap();
    });

    app.unwrap().on_copy_to_clipboard(move |text| {
        let mut clipboard = Clipboard::new().expect("Failed to initialize clipboard");
        clipboard
            .set_text(text.to_string())
            .expect("Failed to copy text to clipboard");
        log::info!("Copied to clipboard: {}", text);
    });

    app.unwrap().on_update_scan_range(move || {
        let begin_slider_position = app.unwrap().get_current_begin_slider_position();
        let end_slider_position = app.unwrap().get_current_end_slider_position();
        if let Ok(max_height) = app.unwrap().get_loaded_header().parse::<u64>() {
            let begin_height = ((begin_slider_position / 100.0) * max_height as f32).round() as u64;
            let end_height = ((end_slider_position / 100.0) * max_height as f32).round() as u64;
            let end_height = end_height.max(begin_height);

            app.unwrap()
                .set_scan_begin_height(begin_height.to_string().into());
            app.unwrap()
                .set_scan_end_height(end_height.to_string().into());
        }
    });

    let ui_input_tx_clone = ui_input_tx.clone();
    let app = main_window.as_weak();

    app.unwrap().on_request_blocks(move || {
        if let Ok(max_height) = app.unwrap().get_loaded_header().parse::<u64>() {
            let begin_height = app
                .unwrap()
                .get_scan_begin_height()
                .parse::<u64>()
                .unwrap_or(0);
            let end_height = app
                .unwrap()
                .get_scan_end_height()
                .parse::<u64>()
                .unwrap_or(max_height);
            let end_height = end_height.max(begin_height);

            if end_height >= begin_height {
                let range = MerkleScanRange {
                    begin: begin_height,
                    end: end_height,
                };
                log::info!("Requesting blocks: {:?}", range);
                ui_input_tx_clone
                    .send(UIMessage::RequestBlocks(range))
                    .unwrap();
            }
        }
    });
    let app = main_window.as_weak();

    app.unwrap().on_preset_last_10k(move || {
        if let Ok(max_height) = app.unwrap().get_loaded_header().parse::<u64>() {
            let end_height = max_height;
            let begin_height = max_height.saturating_sub(10_000);

            app.unwrap().set_current_begin_slider_position(
                (begin_height as f32 / max_height as f32) * 100.0,
            );
            app.unwrap().set_current_end_slider_position(100.0);
            app.unwrap()
                .set_scan_begin_height(begin_height.to_string().into());
            app.unwrap()
                .set_scan_end_height(end_height.to_string().into());
        }
    });
    let app = main_window.as_weak();

    app.unwrap().on_preset_last_100k(move || {
        if let Ok(max_height) = app.unwrap().get_loaded_header().parse::<u64>() {
            let end_height = max_height;
            let begin_height = max_height.saturating_sub(100_000);

            app.unwrap().set_current_begin_slider_position(
                (begin_height as f32 / max_height as f32) * 100.0,
            );
            app.unwrap().set_current_end_slider_position(100.0);
            app.unwrap()
                .set_scan_begin_height(begin_height.to_string().into());
            app.unwrap()
                .set_scan_end_height(end_height.to_string().into());
        }
    });
    let app = main_window.as_weak();

    app.unwrap().on_preset_full_chain(move || {
        if let Ok(max_height) = app.unwrap().get_loaded_header().parse::<u64>() {
            let end_height = max_height;
            let begin_height = 0;

            app.unwrap().set_current_begin_slider_position(0.0);
            app.unwrap().set_current_end_slider_position(100.0);
            app.unwrap()
                .set_scan_begin_height(begin_height.to_string().into());
            app.unwrap()
                .set_scan_end_height(end_height.to_string().into());
        }
    });


    let app = main_window.as_weak();
    std::thread::spawn(move || {
        loop {
            chan::select! {
                recv(ui_show_rx) -> event => {
                    if let Ok(event) = event {
                        app.upgrade_in_event_loop(move |app| {
                            match event {
                                UIMessage::HeaderLoaded(height) => {
                                    app.set_loaded_header(height.to_string().into());
                                    app.invoke_update_scan_range();
                                }
                                UIMessage::BlockConnected(height) => {
                                    app.set_loaded_header(height.to_string().into());
                                    app.invoke_update_scan_range();
                                }
                                UIMessage::AddBloomItem(item) => {
                                    app.set_bloom_item(item.into());
                                }
                                UIMessage::ReceivedBlock(item) => {
                                    app.set_latest_merkle(item.to_string().into());
                                }
                                UIMessage::BlocksDownloading(is_scanning) => {
                                    // let end_height = app.get_scan_end_height().to_string();
                                    // let end_height: u64 = match end_height.parse() {
                                    //     Ok(num) => num,
                                    //     Err(_) => unreachable!(),
                                    // };
                                    // if end_height == height {
                                         app.set_is_scanning(is_scanning);
                                    // } else {
                                         // app.set_is_scanning(true);

                                    // }
                                }
                                UIMessage::NetworkConnected(network) => {
                                    match network {
                                        Network::Chipnet => {
                                            app.set_network("chipnet".into());
                                        },
                                        _ => {
                                            app.set_network("mainnet".into());
                                        },
                                    }
                                }
                                UIMessage::ResetFilter | UIMessage::ClearFilterAndPeers => {
                                    app.set_bloom_items(ModelRc::new(slint::VecModel::from(vec![])));
                                    app.set_filtered_peers(ModelRc::new(slint::VecModel::from(vec![])));
                                }
                                UIMessage::PeerLoadedFilter(item) => {
                                    let peers:Vec<SharedString> = app.get_filtered_peers().iter().collect();
                                    let peers_model =
                                    std::rc::Rc::new(slint::VecModel::from(peers));
                                    peers_model.push(item.to_string().into());
                                    app.set_filtered_peers(peers_model .clone().into());
                                }
                                UIMessage::ReceivedMatchedTx { transaction } => {
                                    let txid = transaction.txid().to_string();
                                    app.set_matched_tx(txid.clone().into());
                                    let txs:Vec<SharedString> = app.get_matched_txs().iter().collect();
                                    let txs_model = std::rc::Rc::new(slint::VecModel::from(txs));
                                    let mut seen = std::collections::HashSet::new();
                                    txs_model.iter().for_each(|i| {
                                        seen.insert(i.as_str().to_string());
                                    });
                                    if seen.insert(txid.clone()) {
                                        txs_model.push(txid.into());
                                    }
                                    app.set_matched_txs(txs_model.clone().into());
                                }
                                _ => {},
                            }
                        }).unwrap();
                    }
                }
            }
        }
    });

    main_window.run().unwrap();
      Ok(())
}

impl Options {
    pub fn from_env() -> Self {
        argh::from_env()
    }
}
