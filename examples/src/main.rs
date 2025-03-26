use hex;
use std::ops::ControlFlow;
use std::{env, io, net, thread};
mod error;
use nakamoto_cash::chain::Transaction;
use nakamoto_cash::common::bitcoin::network::constants::ServiceFlags;
// use std::str::Lines;
use arboard::Clipboard;
use slint::{Model, ModelRc, SharedString};
use std::rc::Rc;

use slint::PlatformError;

slint::include_modules!();

use argh::FromArgs;
use client::{Client, Config, Event, Loading, Peer};
mod logger;
use nakamoto_cash::client::traits::Handle;
use nakamoto_cash::client::{self, Network};
use nakamoto_cash::common::bitcoin::util::bloom::{Bloom, BloomFilter};
use nakamoto_cash::p2p::Command;
use nakamoto_cash::p2p::PeerId;
type Reactor = nakamoto_cash::net::poll::Reactor<net::TcpStream>;
use crossbeam_channel::{self as chan, Receiver, Sender};
// mod bloom;
// use bloom::{Bloom, BloomFilter};
use std::io::Write;

#[derive(Clone, Debug)]
pub enum UIMessage {
    HeaderLoaded(u64),
    BlockConnected(u64),
    AddBlooomItem(String),
    SendLoadFilter,
    PeerLoadedFilter(PeerId),
    ReceivedMatchedTx { transaction: Transaction },
}

pub struct Watcher<H> {
    client: H,
    network: Network,
    peers: Vec<Peer>,
    bloom: BloomFilter,
}

impl<H: Handle> Watcher<H> {
    pub fn new(client: H, network: client::Network) -> Self {
        let filter = Bloom::<u8>::new_for_fp_rate(10_000, 0.01);
        let bloom = BloomFilter::from(filter);
        let peers: Vec<Peer> = Vec::with_capacity(32);
        Self {
            client,
            network,
            peers,
            bloom,
        }
    }

    /// Run the watch loop until it exits.
    pub fn run(
        &mut self,
        events: &Receiver<client::Event>,
        ui_input_rx: &Receiver<UIMessage>,
        ui_show_tx: &Sender<UIMessage>,
        peers_vec_tx: &Sender<Vec<Peer>>,
        peers_vec_rx: &Receiver<Vec<Peer>>,
    ) -> Result<(), error::Error> {
        loop {
            // Try UI input
            if let Ok(event) = ui_input_rx.try_recv() {
                if let ControlFlow::Break(()) =
                    self.handle_user_input(event, peers_vec_tx, peers_vec_rx)?
                {
                    break;
                }
            }
            // Try events
            if let Ok(event) = events.try_recv() {
                if let ControlFlow::Break(()) = self.handle_client_event(event, ui_show_tx)? {
                    break;
                }
            }
            // Small sleep to prevent tight looping if all channels are empty
            // thread::sleep(std::time::Duration::from_millis(1));
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
                _ = ui_show_tx.send(UIMessage::HeaderLoaded(tip));
                log::info!("Client Ready @{:?}", time);
            }
            Event::BlockConnected { height, .. } => {
                _ = ui_show_tx.send(UIMessage::BlockConnected(height))
            }
            Event::ReceivedMatchedTx { transaction } => {
                _ = ui_show_tx.send(UIMessage::ReceivedMatchedTx { transaction });
            }
            Event::PeerLoadedBloomFilter { peer,.. } => {
                log::info!("PeerLoadedFilter ??{:?}", peer);

                ui_show_tx.send(UIMessage::PeerLoadedFilter(peer) );
            }
            _ => {} // Added catch-all for unhandled events
        }
        Ok(ControlFlow::Continue(()))
    }
    fn send_bloom_filter(&mut self, peers: Vec<PeerId>) -> Result<ControlFlow<()>, error::Error> {
        let filter = self.bloom.clone();
        _ = self
            .client
            .command(Command::LoadBloomFilter((filter, peers)));

        Ok(ControlFlow::Continue(()))
    }

    fn handle_user_input(
        &mut self,
        ui_input: UIMessage,
        peers_vec_tx: &Sender<Vec<Peer>>,
        peers_vec_rx: &Receiver<Vec<Peer>>,
    ) -> Result<ControlFlow<()>, error::Error> {
        match ui_input {
            UIMessage::AddBlooomItem(data) => {
                let mut temp_bloom = Bloom::<u8>::new_for_fp_rate(1024, 0.01);
                match hex::decode(data) {
                    Ok(mut bytes) => {
                        temp_bloom.set(&mut bytes);
                    }
                    Err(e) => println!("Error: {}", e),
                }

                self.bloom = BloomFilter::from(temp_bloom);
            }
            UIMessage::SendLoadFilter => {
                let peer_tx = peers_vec_tx.clone();
                _ = self
                    .client
                    .command(Command::GetPeers(ServiceFlags::BLOOM, peer_tx));
                if let Ok(peers) = peers_vec_rx.recv() {
                    let peers = peers.iter().map(|p| p.addr).collect::<Vec<_>>();
                    _ = self.send_bloom_filter(peers);
                }
            }
            _ => {}
        }
        Ok(ControlFlow::Continue(()))
    }
}

// Rest of your code remains the same...
/// A Bitcoin wallet.
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
    let addrs = opts.connect;
    let connect = addrs;

    let level = if opts.debug.is_some() {
        log::Level::Debug
    } else {
        log::Level::Info
    };

    let (loading_tx, loading_rx) = chan::unbounded();
    _ = loading_rx;
    let (ui_show_tx, ui_show_rx) = chan::unbounded();
    let (ui_input_tx, ui_input_rx) = chan::unbounded();
    let (peers_vec_tx, peers_vec_tx_rx) = chan::unbounded();

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
            &peers_vec_tx,
            &peers_vec_tx_rx,
        ) {
            println!("FATAL ERR {:?}", err);
            std::process::exit(1);
        };
    });

    // New thread for handling user input from terminal
    _ = run_ui_main(&ui_input_tx, &ui_show_rx);
    t1.join().unwrap().unwrap();
    t2.join().unwrap();
    // t3.join().unwrap();
}

pub fn run_ui_main(
    ui_input_tx: &Sender<UIMessage>,
    ui_show_rx: &Receiver<UIMessage>,
) -> Result<(), PlatformError> {
    let main_window = MainWindow::new()?;
    let ui_show_rx = ui_show_rx.clone();

    let app = main_window.as_weak().clone();
    let tx_handle = ui_input_tx.clone();

    app.unwrap().on_load_bloom_item(move || {
        let filter_items = Rc::new(slint::VecModel::<SharedString>::from(vec![]));

        let items = app.unwrap().get_bloom_items();
        let item = app.unwrap().get_bloom_item();

        filter_items.push(item);

        items.iter().for_each(|f| {
            filter_items.push(f.clone());
        });
        app.unwrap().set_bloom_items(filter_items.clone().into());

        let items = filter_items
            .iter()
            .map(|i| i.to_string())
            .collect::<Vec<_>>();

        log::info!("BLOOM ITEMS {:?}", items);

        items.iter().for_each(|f| {
            _ = tx_handle.send(UIMessage::AddBlooomItem(f.clone().into()));
        });
    });

    let ui_input_tx = ui_input_tx.clone();
    let app = main_window.as_weak().clone();
    app.unwrap().on_load_peers_filter(move || {
        _ = ui_input_tx.send(UIMessage::SendLoadFilter);
    });

    // Add Clipboard Functionality
    let app = main_window.as_weak().clone();
    app.unwrap().on_copy_to_clipboard(move |text| {
        let mut clipboard = Clipboard::new().expect("Failed to initialize clipboard");
        clipboard
            .set_text(text.to_string())
            .expect("Failed to copy text to clipboard");
        log::info!("Copied to clipboard: {}", text);
    });

    let app = main_window.as_weak().clone();
    std::thread::spawn(move || {
        loop {
            chan::select! {
            recv(ui_show_rx) -> event => {
                if let Ok(event) = event {
                _ = app .upgrade_in_event_loop(move |app| {
                 match event {
                    UIMessage::HeaderLoaded(height) => {
                        app.as_weak().unwrap().set_loaded_header(height.to_string().into());
                    }
                    UIMessage::BlockConnected(height) => {
                        app.as_weak().unwrap().set_loaded_header(height.to_string().into());
                    }
                    UIMessage::AddBlooomItem(item) => {
                        app.as_weak().unwrap().set_bloom_item(item.into())
                    }
                    UIMessage::PeerLoadedFilter(item) => {
                        // app.as_weak().unwrap().set_bloom_item(item.into())
                        let peers = Rc::new(slint::VecModel::<SharedString>::from(vec![]));
                        let peers_current = app.as_weak().unwrap().get_filtered_peers();
                        let new_peer = item.to_string();

                        log::info!("Peer Loaded Filter{:?}", new_peer);
                        app.as_weak().unwrap().set_filtered_peers(peers.clone().into());

                        let mut seen = std::collections::HashSet::new();
                        // Add existing items
                        peers_current.iter().for_each(|i| {
                            if seen.insert(i.as_str().to_string()) {
                                peers.push(i.clone());
                            }
                        });
                            // Add new item if not already present
                        if seen.insert(new_peer.clone()) {
                            // let item = app.as_weak().unwrap().get_matched_tx();
                            peers.push(new_peer.clone().into());
                        }
                        app.as_weak().unwrap().set_filtered_peers(peers.clone().into());

                    }
                    UIMessage::ReceivedMatchedTx { transaction } => {
                        app.as_weak().unwrap().set_matched_tx(transaction.txid().to_string().into());

                        let tx_items = Rc::new(slint::VecModel::<SharedString>::from(vec![]));
                        let tx_items_current = app.as_weak().unwrap().get_matched_txs();
                        let new_txid = transaction.txid().to_string();

                        log::info!("Received matched tx {:?}", transaction.txid());
                        app.as_weak().unwrap().set_matched_txs(tx_items.clone().into());


                        // Use HashSet with owned Strings instead of &str
                        let mut seen = std::collections::HashSet::new();

                        // Add existing items
                        tx_items_current.iter().for_each(|i| {
                            if seen.insert(i.as_str().to_string()) {
                                tx_items.push(i.clone());
                            }
                        });

                        // Add new item if not already present
                        if seen.insert(new_txid.clone()) {
                            let item = app.as_weak().unwrap().get_matched_tx();
                            tx_items.push(item);
                        }

                        app.as_weak().unwrap().set_matched_txs(tx_items.clone().into());
                    }
                    _ => {},
                }

                });
              }
               }
            }
        }
    });

    main_window.run()
}

impl Options {
    pub fn from_env() -> Self {
        argh::from_env()
    }
}
